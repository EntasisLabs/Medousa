//! Host-bus identity tools: read context, propose patches, commit under policy (AX-4c).

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use schemars::schema::{InstanceType, Schema, SchemaObject};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stasis::application::use_cases::identity_memory_service::IdentityMemoryService;
use stasis::domain::errors::{Result as StasisResult, StasisError};
use stasis::ports::outbound::memory::identity_memory_models::{
    ChannelProfileEntity, CommitEntityUpdateRequest, CommitEntityUpdateResponse, CommitOutcomeCode,
    ContactEntity, FlattenedPolicyClaim, GetIdentityContextResponse, IdentityContextMode,
    IdentityEntityType, PersonaEntity, PolicyProfileEntity, ProposeEntityUpdateRequest,
    ProposeEntityUpdateResponse, RelationshipEntity, RelationshipStatus, UpdateSource, UpdateTier,
    UserEntity,
};
use tokio::sync::mpsc;

use crate::events::TuiEvent;
use crate::identity_memory::{
    build_identity_context_request, resolve_identity_channel_id, resolve_identity_persona_id,
};
use stasis::ports::outbound::memory::memory_context_writer::MemoryContextWriter;

use crate::cognitive_identity::{
    IdentityRecallHit, load_cognitive_identity_snapshot, recall_identity_facts,
};
use crate::cognitive_identity_writer::{
    CognitiveFactKind, CognitiveIdentityWriter, attributes_map_to_tags,
    maybe_store_identity_sttp_bridge,
};
use crate::identity_store_ext::MedousaIdentityMemoryStore;
use crate::identity_write_policy::{
    evaluate_identity_commit, load_identity_product_config, parse_identity_entity_type,
    parse_update_source,
};
use crate::semantic_values::{RequiredContent, TrimmedText};
use crate::typed_tools::{CompatList, CompatOption, ToolId, medousa_tool};

const COGNITION_IDENTITY_CONTEXT_ID: ToolId = ToolId::new("cognition_identity_context");
const COGNITION_IDENTITY_PROPOSE_ID: ToolId = ToolId::new("cognition_identity_propose");
const COGNITION_IDENTITY_RECALL_ID: ToolId = ToolId::new("cognition_identity_recall");
const COGNITION_IDENTITY_REMEMBER_ID: ToolId = ToolId::new("cognition_identity_remember");
const COGNITION_IDENTITY_COMMIT_ID: ToolId = ToolId::new("cognition_identity_commit");

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub(crate) struct CompatibleObject(Value);

impl CompatibleObject {
    fn as_value(&self) -> &Value {
        &self.0
    }

    pub(crate) fn into_value(self) -> Value {
        self.0
    }
}

impl From<Value> for CompatibleObject {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

impl JsonSchema for CompatibleObject {
    fn schema_name() -> String {
        "CompatibleObject".to_string()
    }

    fn is_referenceable() -> bool {
        false
    }

    fn json_schema(_: &mut schemars::r#gen::SchemaGenerator) -> Schema {
        Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::Object.into()),
            ..SchemaObject::default()
        })
    }
}

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum IdentityContextModeSchema {
    Full,
    Policy,
    Cognitive,
}

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum IdentityUpdateSourceSchema {
    UserDirect,
    ModelInferred,
    SystemEvent,
}

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum IdentityRecallFactKindSchema {
    Preference,
    Person,
    Note,
    Any,
}

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum IdentityFactKindSchema {
    Preference,
    Person,
    Note,
}

#[allow(dead_code)]
#[derive(Debug, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum IdentityUpdateTierSchema {
    AutoCommit,
    ConfirmRequired,
    ApprovalRequired,
}

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

fn required_identity_text(value: Option<String>, field: &str) -> StasisResult<TrimmedText> {
    let value = value.ok_or_else(|| StasisError::PortFailure(format!("{field} is required")))?;
    TrimmedText::new(value).map_err(|_| StasisError::PortFailure(format!("{field} is required")))
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

fn resolve_effective_identity_user_id(
    requested_user_id: Option<&str>,
    default_user_id: &str,
    workshop_dynamic: bool,
) -> String {
    optional_str(requested_user_id).unwrap_or_else(|| {
        if workshop_dynamic {
            crate::user_profiles::resolve_workshop_identity_user_id()
        } else {
            default_user_id.to_string()
        }
    })
}

pub struct CognitionIdentityContextTool {
    service: Arc<IdentityMemoryService>,
    default_user_id: String,
    default_persona_id: String,
    default_channel_id: String,
    workshop_dynamic: bool,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionIdentityContextTool {
    pub fn new(
        service: Arc<IdentityMemoryService>,
        default_user_id: String,
        default_persona_id: String,
        default_channel_id: String,
        workshop_dynamic: bool,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            service,
            default_user_id,
            default_persona_id,
            default_channel_id,
            workshop_dynamic,
            event_tx,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdentityContextInput {
    /// Override identity user id
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) user_id: CompatOption<String>,
    /// Override persona id
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) persona_id: CompatOption<String>,
    /// Override channel id
    #[serde(default)]
    #[schemars(
        with = "String",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) channel_id: CompatOption<String>,
    #[serde(default)]
    #[schemars(
        with = "usize",
        range(min = 1, max = 64),
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) relationship_limit: CompatOption<usize>,
    /// Identity context slice (default: cognitive)
    #[serde(default)]
    #[schemars(
        with = "IdentityContextModeSchema",
        skip_serializing_if = "crate::typed_tools::CompatOption::is_none"
    )]
    pub(crate) mode: CompatOption<String>,
}

#[derive(Debug)]
struct IdentityContextCommand {
    user_id: Option<TrimmedText>,
    persona_id: Option<TrimmedText>,
    channel_id: Option<TrimmedText>,
    relationship_limit: usize,
    mode: IdentityContextMode,
}

impl TryFrom<IdentityContextInput> for IdentityContextCommand {
    type Error = StasisError;

    fn try_from(input: IdentityContextInput) -> Result<Self, Self::Error> {
        let user_id = input.user_id.into_option();
        let persona_id = input.persona_id.into_option();
        let channel_id = input.channel_id.into_option();
        let relationship_limit = input.relationship_limit.into_option();
        let mode = input.mode.into_option();
        Ok(Self {
            user_id: user_id.and_then(|value| TrimmedText::new(value).ok()),
            persona_id: persona_id.and_then(|value| TrimmedText::new(value).ok()),
            channel_id: channel_id.and_then(|value| TrimmedText::new(value).ok()),
            relationship_limit: relationship_limit.unwrap_or(8).clamp(1, 64),
            mode: parse_identity_context_mode(mode.as_deref())?,
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityPersonaOutput {
    persona_id: String,
    display_name: String,
    status: String,
    version: i32,
    updated_at: DateTime<Utc>,
}

impl From<PersonaEntity> for IdentityPersonaOutput {
    fn from(value: PersonaEntity) -> Self {
        Self {
            persona_id: value.persona_id,
            display_name: value.display_name,
            status: value.status,
            version: value.version,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityUserOutput {
    user_id: String,
    timezone: String,
    language_variant: Option<String>,
    preferences: BTreeMap<String, Value>,
    status: String,
    version: i32,
    updated_at: DateTime<Utc>,
}

impl From<UserEntity> for IdentityUserOutput {
    fn from(value: UserEntity) -> Self {
        Self {
            user_id: value.user_id,
            timezone: value.timezone,
            language_variant: value.language_variant,
            preferences: value.preferences,
            status: value.status,
            version: value.version,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityContactOutput {
    contact_id: String,
    display_name: String,
    aliases: Vec<String>,
    status: String,
    version: i32,
    updated_at: DateTime<Utc>,
}

impl From<ContactEntity> for IdentityContactOutput {
    fn from(value: ContactEntity) -> Self {
        Self {
            contact_id: value.contact_id,
            display_name: value.display_name,
            aliases: value.aliases,
            status: value.status,
            version: value.version,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityChannelOutput {
    channel_id: String,
    channel_type: String,
    proactive_allowed: bool,
    status: String,
    version: i32,
    updated_at: DateTime<Utc>,
}

impl From<ChannelProfileEntity> for IdentityChannelOutput {
    fn from(value: ChannelProfileEntity) -> Self {
        Self {
            channel_id: value.channel_id,
            channel_type: value.channel_type,
            proactive_allowed: value.proactive_allowed,
            status: value.status,
            version: value.version,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityEntityRefOutput {
    entity_type: String,
    entity_id: String,
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityAutonomyScopeOutput {
    allow: Vec<String>,
    deny: Vec<String>,
    approval_required: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityInterruptionPolicyOutput {
    quiet_hours: Option<String>,
    allow_urgent_only: Option<bool>,
    urgent_threshold: Option<f32>,
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityEscalationPolicyOutput {
    mode: Option<String>,
    fallback: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
enum IdentityRelationshipStatusOutput {
    Proposed,
    Active,
    Suspended,
    Deprecated,
    Revoked,
}

impl From<RelationshipStatus> for IdentityRelationshipStatusOutput {
    fn from(value: RelationshipStatus) -> Self {
        match value {
            RelationshipStatus::Proposed => Self::Proposed,
            RelationshipStatus::Active => Self::Active,
            RelationshipStatus::Suspended => Self::Suspended,
            RelationshipStatus::Deprecated => Self::Deprecated,
            RelationshipStatus::Revoked => Self::Revoked,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
enum IdentityUpdateSourceOutput {
    UserDirect,
    ModelInferred,
    SystemEvent,
}

impl From<UpdateSource> for IdentityUpdateSourceOutput {
    fn from(value: UpdateSource) -> Self {
        match value {
            UpdateSource::UserDirect => Self::UserDirect,
            UpdateSource::ModelInferred => Self::ModelInferred,
            UpdateSource::SystemEvent => Self::SystemEvent,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityRelationshipOutput {
    relationship_id: String,
    source_entity_ref: IdentityEntityRefOutput,
    target_entity_ref: IdentityEntityRefOutput,
    relationship_kind: String,
    status: IdentityRelationshipStatusOutput,
    trust_level: f32,
    confidence: f32,
    strength_score: f32,
    recency_score: f32,
    autonomy_scope: IdentityAutonomyScopeOutput,
    approval_profile_id: Option<String>,
    interruption_policy: IdentityInterruptionPolicyOutput,
    escalation_policy: IdentityEscalationPolicyOutput,
    policy_tags: Vec<String>,
    provenance: IdentityUpdateSourceOutput,
    parent_relationship_id: Option<String>,
    governing_relationship_ids: Vec<String>,
    derived_from_relationship_id: Option<String>,
    last_transition_reason: Option<String>,
    transition_receipt_id: Option<String>,
    version: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<RelationshipEntity> for IdentityRelationshipOutput {
    fn from(value: RelationshipEntity) -> Self {
        Self {
            relationship_id: value.relationship_id,
            source_entity_ref: IdentityEntityRefOutput {
                entity_type: value.source_entity_ref.entity_type,
                entity_id: value.source_entity_ref.entity_id,
            },
            target_entity_ref: IdentityEntityRefOutput {
                entity_type: value.target_entity_ref.entity_type,
                entity_id: value.target_entity_ref.entity_id,
            },
            relationship_kind: value.relationship_kind.as_str().to_string(),
            status: value.status.into(),
            trust_level: value.trust_level,
            confidence: value.confidence,
            strength_score: value.strength_score,
            recency_score: value.recency_score,
            autonomy_scope: IdentityAutonomyScopeOutput {
                allow: value.autonomy_scope.allow,
                deny: value.autonomy_scope.deny,
                approval_required: value.autonomy_scope.approval_required,
            },
            approval_profile_id: value.approval_profile_id,
            interruption_policy: IdentityInterruptionPolicyOutput {
                quiet_hours: value.interruption_policy.quiet_hours,
                allow_urgent_only: value.interruption_policy.allow_urgent_only,
                urgent_threshold: value.interruption_policy.urgent_threshold,
            },
            escalation_policy: IdentityEscalationPolicyOutput {
                mode: value.escalation_policy.mode,
                fallback: value.escalation_policy.fallback,
            },
            policy_tags: value.policy_tags,
            provenance: value.provenance.into(),
            parent_relationship_id: value.parent_relationship_id,
            governing_relationship_ids: value.governing_relationship_ids,
            derived_from_relationship_id: value.derived_from_relationship_id,
            last_transition_reason: value.last_transition_reason,
            transition_receipt_id: value.transition_receipt_id,
            version: value.version,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityPolicyProfileOutput {
    policy_profile_id: String,
    graph_max_depth: usize,
    trust_delta_max_per_window: f32,
    status: String,
    version: i32,
    updated_at: DateTime<Utc>,
}

impl From<PolicyProfileEntity> for IdentityPolicyProfileOutput {
    fn from(value: PolicyProfileEntity) -> Self {
        Self {
            policy_profile_id: value.policy_profile_id,
            graph_max_depth: value.graph_max_depth,
            trust_delta_max_per_window: value.trust_delta_max_per_window,
            status: value.status,
            version: value.version,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
struct IdentityFlattenedClaimOutput {
    claim_id: String,
    source_relationship_ids: Vec<String>,
    summary: String,
    confidence: f32,
    timestamp: DateTime<Utc>,
}

impl From<FlattenedPolicyClaim> for IdentityFlattenedClaimOutput {
    fn from(value: FlattenedPolicyClaim) -> Self {
        Self {
            claim_id: value.claim_id,
            source_relationship_ids: value.source_relationship_ids,
            summary: value.summary,
            confidence: value.confidence,
            timestamp: value.timestamp,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct IdentityContextOutput {
    persona: Option<IdentityPersonaOutput>,
    user: Option<IdentityUserOutput>,
    channel: Option<IdentityChannelOutput>,
    contacts: Vec<IdentityContactOutput>,
    relationships: Vec<IdentityRelationshipOutput>,
    policy_profiles: Vec<IdentityPolicyProfileOutput>,
    graph_depth_used: usize,
    flattened_claims: Vec<IdentityFlattenedClaimOutput>,
}

impl From<GetIdentityContextResponse> for IdentityContextOutput {
    fn from(value: GetIdentityContextResponse) -> Self {
        Self {
            persona: value.persona.map(Into::into),
            user: value.user.map(Into::into),
            channel: value.channel.map(Into::into),
            contacts: value.contacts.into_iter().map(Into::into).collect(),
            relationships: value.relationships.into_iter().map(Into::into).collect(),
            policy_profiles: value.policy_profiles.into_iter().map(Into::into).collect(),
            graph_depth_used: value.graph_depth_used,
            flattened_claims: value.flattened_claims.into_iter().map(Into::into).collect(),
        }
    }
}

#[medousa_tool(id = COGNITION_IDENTITY_CONTEXT_ID)]
impl CognitionIdentityContextTool {
    /// Read identity graph context (persona, user, channels, relationships) for this turn.
    pub(crate) async fn invoke_typed(
        &self,
        input: IdentityContextInput,
    ) -> stasis::prelude::Result<IdentityContextOutput> {
        let command = IdentityContextCommand::try_from(input)?;
        emit_invoked(
            &self.event_tx,
            COGNITION_IDENTITY_CONTEXT_ID.as_str(),
            "identity context",
        )
        .await;
        let user_id = resolve_effective_identity_user_id(
            command.user_id.as_ref().map(TrimmedText::as_str),
            &self.default_user_id,
            self.workshop_dynamic,
        );
        let persona_id = command
            .persona_id
            .map(TrimmedText::into_string)
            .unwrap_or_else(|| self.default_persona_id.clone());
        let channel_id = command
            .channel_id
            .map(TrimmedText::into_string)
            .unwrap_or_else(|| self.default_channel_id.clone());
        let relationship_limit = command.relationship_limit;
        let mode = command.mode;

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

        Ok(response.into())
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

#[derive(Debug, JsonSchema)]
pub struct IdentityProposeInput {
    /// persona | user | contact | relationship | channel | policy
    #[schemars(required, with = "String")]
    pub(crate) entity_type: Option<String>,
    #[schemars(required, with = "String")]
    pub(crate) entity_id: Option<String>,
    /// Flat or nested JSON patch object
    #[schemars(required, with = "CompatibleObject")]
    pub(crate) patch: Option<CompatibleObject>,
    #[schemars(
        with = "IdentityUpdateSourceSchema",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
    #[schemars(
        with = "f64",
        range(min = 0, max = 1),
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) confidence: Option<f64>,
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) actor: Option<String>,
    /// RFC3339 UTC
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) expires_at: Option<String>,
}

impl<'de> Deserialize<'de> for IdentityProposeInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            entity_type: CompatOption<String>,
            #[serde(default)]
            entity_id: CompatOption<String>,
            #[serde(default)]
            patch: Option<CompatibleObject>,
            #[serde(default)]
            source: CompatOption<String>,
            #[serde(default)]
            confidence: CompatOption<f64>,
            #[serde(default)]
            reason: CompatOption<String>,
            #[serde(default)]
            actor: CompatOption<String>,
            #[serde(default)]
            expires_at: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            entity_type: input.entity_type.into_option(),
            entity_id: input.entity_id.into_option(),
            patch: input.patch,
            source: input.source.into_option(),
            confidence: input.confidence.into_option(),
            reason: input.reason.into_option(),
            actor: input.actor.into_option(),
            expires_at: input.expires_at.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
enum IdentityUpdateTierOutput {
    AutoCommit,
    ConfirmRequired,
    ApprovalRequired,
}

impl From<UpdateTier> for IdentityUpdateTierOutput {
    fn from(value: UpdateTier) -> Self {
        match value {
            UpdateTier::AutoCommit => Self::AutoCommit,
            UpdateTier::ConfirmRequired => Self::ConfirmRequired,
            UpdateTier::ApprovalRequired => Self::ApprovalRequired,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct IdentityProposeOutput {
    proposal_ids: Vec<String>,
    tiers: Vec<IdentityUpdateTierOutput>,
    requires_approval: bool,
    split_patch: bool,
    policy_notes: Vec<String>,
}

impl From<ProposeEntityUpdateResponse> for IdentityProposeOutput {
    fn from(value: ProposeEntityUpdateResponse) -> Self {
        Self {
            proposal_ids: value.proposal_ids,
            tiers: value.tiers.into_iter().map(Into::into).collect(),
            requires_approval: value.requires_approval,
            split_patch: value.split_patch,
            policy_notes: value.policy_notes,
        }
    }
}

#[medousa_tool(id = COGNITION_IDENTITY_PROPOSE_ID)]
impl CognitionIdentityProposeTool {
    /// Propose a durable identity patch (persona, user, relationship). Returns proposal_ids and tiers; use cognition_identity_mutate action=identity.commit when policy allows.
    pub(crate) async fn invoke_typed(
        &self,
        input: IdentityProposeInput,
    ) -> stasis::prelude::Result<IdentityProposeOutput> {
        let entity_type_raw = input
            .entity_type
            .as_deref()
            .ok_or_else(|| StasisError::PortFailure("entity_type is required".to_string()))?;
        let entity_id = input
            .entity_id
            .as_deref()
            .ok_or_else(|| StasisError::PortFailure("entity_id is required".to_string()))?;
        let patch = input
            .patch
            .map(CompatibleObject::into_value)
            .filter(Value::is_object)
            .ok_or_else(|| StasisError::PortFailure("patch must be a JSON object".to_string()))?;

        let entity_type =
            parse_identity_entity_type(entity_type_raw).map_err(StasisError::PortFailure)?;
        let source =
            parse_update_source(input.source.as_deref()).map_err(StasisError::PortFailure)?;
        let confidence = input
            .confidence
            .map(|v| v as f32)
            .unwrap_or(0.75)
            .clamp(0.0, 1.0);
        let reason = input
            .reason
            .as_deref()
            .unwrap_or("agent identity propose")
            .to_string();
        let actor = input
            .actor
            .as_deref()
            .unwrap_or("medousa-agent")
            .to_string();
        let expires_at = parse_utc_optional(input.expires_at.as_deref(), "expires_at")
            .map_err(StasisError::PortFailure)?;

        emit_invoked(
            &self.event_tx,
            COGNITION_IDENTITY_PROPOSE_ID.as_str(),
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

        Ok(response.into())
    }
}

// ── cognition_identity_recall ─────────────────────────────────────────────────

pub struct CognitionIdentityRecallTool {
    store: Arc<MedousaIdentityMemoryStore>,
    default_user_id: String,
    workshop_dynamic: bool,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionIdentityRecallTool {
    pub fn new(
        store: Arc<MedousaIdentityMemoryStore>,
        default_user_id: String,
        workshop_dynamic: bool,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            store,
            default_user_id,
            workshop_dynamic,
            event_tx,
        }
    }
}

#[derive(Debug, JsonSchema)]
pub struct IdentityRecallInput {
    #[schemars(required, with = "String")]
    pub(crate) query: Option<String>,
    /// Optional filter; defaults to any
    #[schemars(
        with = "IdentityRecallFactKindSchema",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fact_kind: Option<String>,
    #[schemars(
        with = "usize",
        range(min = 1, max = 20),
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) limit: Option<usize>,
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) user_id: Option<String>,
}

impl<'de> Deserialize<'de> for IdentityRecallInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            query: CompatOption<String>,
            #[serde(default)]
            fact_kind: CompatOption<String>,
            #[serde(default)]
            limit: CompatOption<usize>,
            #[serde(default)]
            user_id: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            query: input.query.into_option(),
            fact_kind: input.fact_kind.into_option(),
            limit: input.limit.into_option(),
            user_id: input.user_id.into_option(),
        })
    }
}

#[derive(Debug)]
struct IdentityRecallCommand {
    query: TrimmedText,
    fact_kind: Option<TrimmedText>,
    limit: usize,
    user_id: Option<TrimmedText>,
}

impl TryFrom<IdentityRecallInput> for IdentityRecallCommand {
    type Error = StasisError;

    fn try_from(input: IdentityRecallInput) -> Result<Self, Self::Error> {
        Ok(Self {
            query: required_identity_text(input.query, "query")?,
            fact_kind: input
                .fact_kind
                .and_then(|value| TrimmedText::new(value).ok()),
            limit: input.limit.unwrap_or(8).clamp(1, 20),
            user_id: input.user_id.and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum IdentityRecallOutput {
    Success {
        query: String,
        hits: Vec<IdentityRecallHit>,
        total_candidates: usize,
        store_preferences_count: usize,
        user_version: Option<i32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hint: Option<String>,
    },
    Error {
        query: String,
        hits: Vec<IdentityRecallHit>,
        total_candidates: usize,
        error: String,
    },
}

#[medousa_tool(id = COGNITION_IDENTITY_RECALL_ID)]
impl CognitionIdentityRecallTool {
    /// Search durable identity memory (preferences, people, notes) by keyword. Use when the turn-start digest lacks detail.
    pub(crate) async fn invoke_typed(
        &self,
        input: IdentityRecallInput,
    ) -> stasis::prelude::Result<IdentityRecallOutput> {
        let command = IdentityRecallCommand::try_from(input)?;
        let query = command.query.as_str();
        let fact_kind = command.fact_kind.as_ref().map(TrimmedText::as_str);
        let limit = command.limit;
        let user_id = resolve_effective_identity_user_id(
            command.user_id.as_ref().map(TrimmedText::as_str),
            &self.default_user_id,
            self.workshop_dynamic,
        );

        emit_invoked(&self.event_tx, COGNITION_IDENTITY_RECALL_ID.as_str(), query).await;

        let store_dyn = self.store.clone()
            as Arc<dyn stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore>;
        let snapshot =
            load_cognitive_identity_snapshot(Some(&store_dyn), &user_id, Some("interactive"), 32)
                .await;

        if let Some(err) = snapshot.error {
            return Ok(IdentityRecallOutput::Error {
                query: query.to_string(),
                hits: Vec::new(),
                total_candidates: 0,
                error: err,
            });
        }

        let result = recall_identity_facts(&snapshot, query, fact_kind, limit);
        let hits_empty = result.hits.is_empty();
        let preferences_count = snapshot
            .user
            .as_ref()
            .map(|user| user.preferences.len())
            .unwrap_or(0);
        let hint = if hits_empty && preferences_count > 0 {
            Some(
                "Identity store has preferences but none matched this query — try a broader query or fact_kind=any"
                    .to_string(),
            )
        } else if hits_empty && preferences_count == 0 {
            Some(
                "No preferences in identity store for this user — remember may not have persisted; check Automations → History receipts"
                    .to_string(),
            )
        } else {
            None
        };
        Ok(IdentityRecallOutput::Success {
            query: result.query,
            hits: result.hits,
            total_candidates: result.total_candidates,
            store_preferences_count: preferences_count,
            user_version: snapshot.user.as_ref().map(|user| user.version),
            hint,
        })
    }
}

// ── cognition_identity_remember ───────────────────────────────────────────────

pub struct CognitionIdentityRememberTool {
    writer: Arc<CognitiveIdentityWriter>,
    default_user_id: String,
    workshop_dynamic: bool,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionIdentityRememberTool {
    pub fn new(
        store: Arc<MedousaIdentityMemoryStore>,
        memory_writer: Option<Arc<dyn MemoryContextWriter>>,
        default_user_id: String,
        workshop_dynamic: bool,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            writer: Arc::new(CognitiveIdentityWriter::new(store, memory_writer)),
            default_user_id,
            workshop_dynamic,
            event_tx,
        }
    }
}

fn parse_fact_kind(raw: &str) -> Result<CognitiveFactKind, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "preference" => Ok(CognitiveFactKind::Preference),
        "person" => Ok(CognitiveFactKind::Person),
        "note" => Ok(CognitiveFactKind::Note),
        other => Err(format!(
            "unsupported fact_kind '{other}', expected preference|person|note"
        )),
    }
}

fn parse_attributes_tags(value: Option<&Value>) -> Vec<String> {
    let Some(raw) = value else {
        return Vec::new();
    };
    if let Some(map) = raw.as_object() {
        let btree: std::collections::BTreeMap<String, Value> =
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        return attributes_map_to_tags(&btree);
    }
    if let Some(list) = raw.as_array() {
        return list
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect();
    }
    Vec::new()
}

#[derive(Debug, JsonSchema)]
pub struct IdentityRememberInput {
    /// preference = user key/value; person = contact + relationship; note = preference key or freeform note
    #[schemars(required, with = "IdentityFactKindSchema")]
    pub(crate) fact_kind: Option<String>,
    /// Preference key (beverage), person display name (Mario), or note subject
    #[schemars(required, with = "String")]
    pub(crate) subject: Option<String>,
    /// Human-readable fact, e.g. User prefers matcha over coffee
    #[schemars(required, with = "String")]
    pub(crate) statement: Option<String>,
    /// Optional structured tags for people (role, employer, …) — rendered as policy_tags
    #[schemars(with = "CompatibleObject", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) attributes: Option<CompatibleObject>,
    /// Optional contact aliases (person facts only)
    #[schemars(with = "Vec<String>", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) aliases: Option<Vec<String>>,
    /// Defaults to user_direct when operator stated the fact
    #[schemars(
        with = "IdentityUpdateSourceSchema",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
    #[schemars(
        with = "f64",
        range(min = 0, max = 1),
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) confidence: Option<f64>,
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    /// Override default identity user id
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) user_id: Option<String>,
}

impl<'de> Deserialize<'de> for IdentityRememberInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            fact_kind: CompatOption<String>,
            #[serde(default)]
            subject: CompatOption<String>,
            #[serde(default)]
            statement: CompatOption<String>,
            #[serde(default)]
            attributes: Option<CompatibleObject>,
            #[serde(default)]
            aliases: CompatList<String>,
            #[serde(default)]
            source: CompatOption<String>,
            #[serde(default)]
            confidence: CompatOption<f64>,
            #[serde(default)]
            reason: CompatOption<String>,
            #[serde(default)]
            user_id: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            fact_kind: input.fact_kind.into_option(),
            subject: input.subject.into_option(),
            statement: input.statement.into_option(),
            attributes: input.attributes,
            aliases: input.aliases.into_option(),
            source: input.source.into_option(),
            confidence: input.confidence.into_option(),
            reason: input.reason.into_option(),
            user_id: input.user_id.into_option(),
        })
    }
}

#[derive(Debug)]
struct IdentityRememberCommand {
    fact_kind: CognitiveFactKind,
    fact_kind_label: TrimmedText,
    subject: TrimmedText,
    statement: RequiredContent,
    attributes: Option<CompatibleObject>,
    aliases: Vec<TrimmedText>,
    source: Option<UpdateSource>,
    confidence: Option<f32>,
    reason: Option<RequiredContent>,
    user_id: Option<TrimmedText>,
}

impl TryFrom<IdentityRememberInput> for IdentityRememberCommand {
    type Error = StasisError;

    fn try_from(input: IdentityRememberInput) -> Result<Self, Self::Error> {
        let fact_kind_label = required_identity_text(input.fact_kind, "fact_kind")?;
        let fact_kind =
            parse_fact_kind(fact_kind_label.as_str()).map_err(StasisError::PortFailure)?;
        let subject = required_identity_text(input.subject, "subject")?;
        let statement = input
            .statement
            .ok_or_else(|| StasisError::PortFailure("statement is required".to_string()))
            .and_then(|value| {
                RequiredContent::new(value)
                    .map_err(|_| StasisError::PortFailure("statement is required".to_string()))
            })?;
        let source = match input.source {
            Some(value) => {
                Some(parse_update_source(Some(value.as_str())).map_err(StasisError::PortFailure)?)
            }
            None => None,
        };
        let aliases = input
            .aliases
            .unwrap_or_default()
            .into_iter()
            .filter_map(|value| TrimmedText::new(value).ok())
            .collect();

        Ok(Self {
            fact_kind,
            fact_kind_label,
            subject,
            statement,
            attributes: input.attributes,
            aliases,
            source,
            confidence: input.confidence.map(|value| value as f32),
            reason: input
                .reason
                .and_then(|value| RequiredContent::new(value).ok()),
            user_id: input.user_id.and_then(|value| TrimmedText::new(value).ok()),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct IdentityRememberOutput {
    committed: bool,
    persisted_verified: bool,
    user_version: Option<i32>,
    proposal_ids: Vec<String>,
    requires_confirmation: bool,
    sttp_bridge_stored: bool,
    digest_preview: Option<String>,
    rationale: Option<String>,
    fact_kind: String,
    subject: String,
}

#[medousa_tool(id = COGNITION_IDENTITY_REMEMBER_ID)]
impl CognitionIdentityRememberTool {
    /// Remember a durable personal fact in identity memory (preferences, people, notes). Prefer over cognition_memory_mutate action=memory.store for operator world-model facts.
    pub(crate) async fn invoke_typed(
        &self,
        input: IdentityRememberInput,
    ) -> stasis::prelude::Result<IdentityRememberOutput> {
        let command = IdentityRememberCommand::try_from(input)?;
        let IdentityRememberCommand {
            fact_kind,
            fact_kind_label,
            subject,
            statement,
            attributes,
            aliases,
            source,
            confidence: requested_confidence,
            reason: requested_reason,
            user_id: requested_user_id,
        } = command;
        let source = source.unwrap_or(UpdateSource::UserDirect);
        let confidence = requested_confidence
            .unwrap_or(if source == UpdateSource::UserDirect {
                1.0
            } else {
                0.85
            })
            .clamp(0.0, 1.0);
        let reason = requested_reason
            .map(RequiredContent::into_string)
            .unwrap_or_else(|| statement.as_str().to_string());
        let fact_kind_raw = fact_kind_label.into_string();
        let subject = subject.into_string();
        let statement = statement.into_string();
        let aliases = aliases
            .into_iter()
            .map(TrimmedText::into_string)
            .collect::<Vec<_>>();
        let user_id = resolve_effective_identity_user_id(
            requested_user_id.as_ref().map(TrimmedText::as_str),
            &self.default_user_id,
            self.workshop_dynamic,
        );

        emit_invoked(
            &self.event_tx,
            COGNITION_IDENTITY_REMEMBER_ID.as_str(),
            &format!("{fact_kind_raw}:{subject}"),
        )
        .await;

        let result = match fact_kind {
            CognitiveFactKind::Preference => {
                self.writer
                    .remember_preference(
                        &user_id,
                        &subject,
                        Value::String(statement.clone()),
                        source,
                        confidence,
                        &reason,
                    )
                    .await?
            }
            CognitiveFactKind::Person => {
                let attributes =
                    parse_attributes_tags(attributes.as_ref().map(CompatibleObject::as_value));
                self.writer
                    .remember_contact(
                        &user_id,
                        &subject,
                        &statement,
                        &attributes,
                        &aliases,
                        source,
                        confidence,
                        &reason,
                    )
                    .await?
            }
            CognitiveFactKind::Note => {
                self.writer
                    .remember_note(&user_id, &subject, &statement, source, confidence, &reason)
                    .await?
            }
        };

        Ok(IdentityRememberOutput {
            committed: result.committed,
            persisted_verified: result.persisted_verified,
            user_version: result.user_version,
            proposal_ids: result.proposal_ids,
            requires_confirmation: result.requires_confirmation,
            sttp_bridge_stored: result.sttp_bridge_stored,
            digest_preview: result.digest_preview,
            rationale: result.rationale,
            fact_kind: fact_kind_raw,
            subject,
        })
    }
}

// ── cognition_identity_commit ─────────────────────────────────────────────────

pub struct CognitionIdentityCommitTool {
    service: Arc<IdentityMemoryService>,
    memory_writer: Option<Arc<dyn MemoryContextWriter>>,
    event_tx: mpsc::Sender<TuiEvent>,
}

impl CognitionIdentityCommitTool {
    pub fn new(
        service: Arc<IdentityMemoryService>,
        memory_writer: Option<Arc<dyn MemoryContextWriter>>,
        event_tx: mpsc::Sender<TuiEvent>,
    ) -> Self {
        Self {
            service,
            memory_writer,
            event_tx,
        }
    }
}

#[derive(Debug, JsonSchema)]
pub struct IdentityCommitInput {
    #[schemars(required, with = "String")]
    pub(crate) proposal_id: Option<String>,
    #[schemars(required, with = "i64")]
    pub(crate) expected_version: Option<i64>,
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) approver: Option<String>,
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) entity_type: Option<String>,
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) entity_id: Option<String>,
    #[schemars(with = "CompatibleObject", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) patch: Option<CompatibleObject>,
    #[schemars(with = "String", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) source: Option<String>,
    #[schemars(with = "f64", skip_serializing_if = "Option::is_none")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) confidence: Option<f64>,
    #[schemars(
        with = "IdentityUpdateTierSchema",
        skip_serializing_if = "Option::is_none"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tier: Option<String>,
}

impl<'de> Deserialize<'de> for IdentityCommitInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireInput {
            #[serde(default)]
            proposal_id: CompatOption<String>,
            #[serde(default)]
            expected_version: CompatOption<i64>,
            #[serde(default)]
            approver: CompatOption<String>,
            #[serde(default)]
            entity_type: CompatOption<String>,
            #[serde(default)]
            entity_id: CompatOption<String>,
            #[serde(default)]
            patch: Option<CompatibleObject>,
            #[serde(default)]
            source: CompatOption<String>,
            #[serde(default)]
            confidence: CompatOption<f64>,
            #[serde(default)]
            tier: CompatOption<String>,
        }

        let input = WireInput::deserialize(deserializer)?;
        Ok(Self {
            proposal_id: input.proposal_id.into_option(),
            expected_version: input.expected_version.into_option(),
            approver: input.approver.into_option(),
            entity_type: input.entity_type.into_option(),
            entity_id: input.entity_id.into_option(),
            patch: input.patch,
            source: input.source.into_option(),
            confidence: input.confidence.into_option(),
            tier: input.tier.into_option(),
        })
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub enum IdentityEntityTypeOutput {
    PersonaEntity,
    UserEntity,
    ContactEntity,
    ChannelProfileEntity,
    PolicyProfileEntity,
    RelationshipEntity,
}

impl From<IdentityEntityType> for IdentityEntityTypeOutput {
    fn from(value: IdentityEntityType) -> Self {
        match value {
            IdentityEntityType::PersonaEntity => Self::PersonaEntity,
            IdentityEntityType::UserEntity => Self::UserEntity,
            IdentityEntityType::ContactEntity => Self::ContactEntity,
            IdentityEntityType::ChannelProfileEntity => Self::ChannelProfileEntity,
            IdentityEntityType::PolicyProfileEntity => Self::PolicyProfileEntity,
            IdentityEntityType::RelationshipEntity => Self::RelationshipEntity,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
pub enum IdentityCommitOutcomeOutput {
    Ok,
    StaleState,
    ApprovalRequired,
    PolicyDenied,
    InvalidPatch,
    ExpiredProposal,
    NotFound,
}

impl From<CommitOutcomeCode> for IdentityCommitOutcomeOutput {
    fn from(value: CommitOutcomeCode) -> Self {
        match value {
            CommitOutcomeCode::Ok => Self::Ok,
            CommitOutcomeCode::StaleState => Self::StaleState,
            CommitOutcomeCode::ApprovalRequired => Self::ApprovalRequired,
            CommitOutcomeCode::PolicyDenied => Self::PolicyDenied,
            CommitOutcomeCode::InvalidPatch => Self::InvalidPatch,
            CommitOutcomeCode::ExpiredProposal => Self::ExpiredProposal,
            CommitOutcomeCode::NotFound => Self::NotFound,
        }
    }
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum IdentityCommitOutput {
    Committed {
        committed: bool,
        code: Option<IdentityCommitOutcomeOutput>,
        entity_type: Option<IdentityEntityTypeOutput>,
        entity_id: Option<String>,
        new_version: Option<i32>,
        receipt_id: Option<String>,
        transition_event_id: Option<String>,
        sttp_bridge_node: Option<String>,
        sttp_bridge_reason: Option<String>,
        rationale: Option<String>,
        sttp_bridge_stored: bool,
    },
    PolicyDenied {
        committed: bool,
        policy_denied: bool,
        rationale: Option<String>,
    },
}

impl IdentityCommitOutput {
    fn from_response(response: CommitEntityUpdateResponse, sttp_bridge_stored: bool) -> Self {
        Self::Committed {
            committed: response.committed,
            code: response.code.map(Into::into),
            entity_type: response.entity_type.map(Into::into),
            entity_id: response.entity_id,
            new_version: response.new_version,
            receipt_id: response.receipt_id,
            transition_event_id: response.transition_event_id,
            sttp_bridge_node: response.sttp_bridge_node,
            sttp_bridge_reason: response.sttp_bridge_reason,
            rationale: response.rationale,
            sttp_bridge_stored,
        }
    }
}

#[medousa_tool(id = COGNITION_IDENTITY_COMMIT_ID)]
impl CognitionIdentityCommitTool {
    /// Commit a proposed identity patch when tier and Medousa policy allow. Pass expected_version from context; set approver for approval_required tiers.
    pub(crate) async fn invoke_typed(
        &self,
        input: IdentityCommitInput,
    ) -> stasis::prelude::Result<IdentityCommitOutput> {
        let proposal_id = input
            .proposal_id
            .as_deref()
            .ok_or_else(|| StasisError::PortFailure("proposal_id is required".to_string()))?;
        let expected_version = input
            .expected_version
            .ok_or_else(|| StasisError::PortFailure("expected_version is required".to_string()))?
            as i32;
        let approver = optional_str(input.approver.as_deref());

        emit_invoked(
            &self.event_tx,
            COGNITION_IDENTITY_COMMIT_ID.as_str(),
            proposal_id,
        )
        .await;

        let config = load_identity_product_config();

        if let (Some(entity_type_raw), Some(entity_id), Some(patch)) = (
            input.entity_type.as_deref(),
            input.entity_id.as_deref(),
            input.patch.as_ref(),
        ) && patch.as_value().is_object()
        {
            let entity_type =
                parse_identity_entity_type(entity_type_raw).map_err(StasisError::PortFailure)?;
            let source =
                parse_update_source(input.source.as_deref()).map_err(StasisError::PortFailure)?;
            let confidence = input.confidence.map(|v| v as f32).unwrap_or(0.75);
            let tier = input
                .tier
                .as_deref()
                .map(|raw| match raw {
                    "confirm_required" => UpdateTier::ConfirmRequired,
                    "approval_required" => UpdateTier::ApprovalRequired,
                    _ => UpdateTier::AutoCommit,
                })
                .unwrap_or(UpdateTier::AutoCommit);

            let proposal_req = ProposeEntityUpdateRequest {
                entity_type,
                entity_id: entity_id.to_string(),
                patch: patch.as_value().clone(),
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
                return Ok(IdentityCommitOutput::PolicyDenied {
                    committed: false,
                    policy_denied: true,
                    rationale: gate.reason,
                });
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

        let sttp_bridge_stored = if response.committed {
            maybe_store_identity_sttp_bridge(self.memory_writer.as_ref(), &config, &response)
                .await
                .unwrap_or(false)
        } else {
            false
        };

        Ok(IdentityCommitOutput::from_response(
            response,
            sttp_bridge_stored,
        ))
    }
}

pub fn default_identity_tool_ids(
    session_user_id: Option<&str>,
    policy_profile: Option<&str>,
) -> (String, String, String) {
    let user_id = session_user_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id);
    let persona_id = resolve_identity_persona_id();
    let channel_id = resolve_identity_channel_id(policy_profile);
    (user_id, persona_id, channel_id)
}

#[cfg(test)]
mod remember_tests {
    use super::*;
    use crate::cognitive_identity::{
        compile_relational_memory_digest, load_cognitive_identity_snapshot,
    };
    use crate::identity_memory::{build_seeded_medousa_identity_store, resolve_identity_user_id};
    use serde_json::json;
    use stasis::application::orchestration::tool_registry::StasisTool;
    use stasis::ports::outbound::memory::identity_memory_store::IdentityMemoryStore;
    use tokio::sync::mpsc;

    #[test]
    fn typed_identity_outputs_preserve_stasis_wire_shapes() {
        let context = GetIdentityContextResponse::default();
        assert_eq!(
            serde_json::to_value(IdentityContextOutput::from(context.clone()))
                .expect("typed context"),
            serde_json::to_value(context).expect("stasis context")
        );

        let proposed = ProposeEntityUpdateResponse::default();
        assert_eq!(
            serde_json::to_value(IdentityProposeOutput::from(proposed.clone()))
                .expect("typed proposal"),
            serde_json::to_value(proposed).expect("stasis proposal")
        );

        let committed = CommitEntityUpdateResponse::default();
        let mut expected = serde_json::to_value(committed.clone()).expect("stasis commit");
        expected
            .as_object_mut()
            .expect("commit object")
            .insert("sttp_bridge_stored".to_string(), Value::Bool(false));
        assert_eq!(
            serde_json::to_value(IdentityCommitOutput::from_response(committed, false))
                .expect("typed commit"),
            expected
        );
    }

    #[tokio::test]
    async fn recall_tool_finds_remembered_preference_and_person() {
        let store = build_seeded_medousa_identity_store().expect("store");
        let user_id = resolve_identity_user_id(None);
        let (event_tx, _rx) = mpsc::channel(4);
        let remember = CognitionIdentityRememberTool::new(
            store.clone(),
            None,
            user_id.clone(),
            false,
            event_tx.clone(),
        );
        remember
            .invoke(json!({
                "fact_kind": "preference",
                "subject": "beverage",
                "statement": "matcha",
                "source": "user_direct"
            }))
            .await
            .expect("remember pref");
        remember
            .invoke(json!({
                "fact_kind": "person",
                "subject": "Mario",
                "statement": "Mario is an engineer at Google",
                "attributes": { "role": "engineer" },
                "source": "user_direct"
            }))
            .await
            .expect("remember person");

        let recall = CognitionIdentityRecallTool::new(store, user_id, false, event_tx);
        let result = recall
            .invoke(json!({ "query": "Mario", "limit": 5 }))
            .await
            .expect("recall");
        let hits = result.get("hits").and_then(Value::as_array).expect("hits");
        assert!(!hits.is_empty());

        let pref = recall
            .invoke(json!({ "query": "matcha", "fact_kind": "preference" }))
            .await
            .expect("pref recall");
        let pref_hits = pref.get("hits").and_then(Value::as_array).expect("hits");
        assert!(!pref_hits.is_empty());
    }

    #[tokio::test]
    async fn remember_tool_writes_preference_and_person_into_digest() {
        let store = build_seeded_medousa_identity_store().expect("store");
        let user_id = resolve_identity_user_id(None);
        let (event_tx, _rx) = mpsc::channel(4);
        let tool = CognitionIdentityRememberTool::new(
            store.clone(),
            None,
            user_id.clone(),
            false,
            event_tx,
        );

        let pref = tool
            .invoke_typed(IdentityRememberInput {
                fact_kind: Some("preference".to_string()),
                subject: Some("beverage".to_string()),
                statement: Some("matcha".to_string()),
                attributes: None,
                aliases: None,
                source: Some("user_direct".to_string()),
                confidence: None,
                reason: None,
                user_id: None,
            })
            .await
            .expect("preference remember");
        assert!(pref.committed);

        let person = tool
            .invoke_typed(IdentityRememberInput {
                fact_kind: Some("person".to_string()),
                subject: Some("Mario".to_string()),
                statement: Some("Mario is an engineer at Google".to_string()),
                attributes: Some(CompatibleObject(json!({
                    "role": "engineer",
                    "employer": "google"
                }))),
                aliases: None,
                source: Some("user_direct".to_string()),
                confidence: None,
                reason: None,
                user_id: None,
            })
            .await
            .expect("person remember");
        assert!(person.committed || person.requires_confirmation);

        let store_dyn = store as Arc<dyn IdentityMemoryStore>;
        let snapshot =
            load_cognitive_identity_snapshot(Some(&store_dyn), &user_id, Some("interactive"), 8)
                .await;
        let digest = compile_relational_memory_digest(&snapshot, 800);
        assert!(digest.contains("matcha"), "digest: {digest}");
        assert!(digest.contains("Mario"), "digest: {digest}");
    }

    #[test]
    fn identity_commands_normalize_fact_inputs_and_preserve_statements() {
        let recall = IdentityRecallCommand::try_from(IdentityRecallInput {
            query: Some("  matcha  ".to_string()),
            fact_kind: Some(" preference ".to_string()),
            limit: Some(999),
            user_id: Some(" user-a ".to_string()),
        })
        .expect("recall command");
        assert_eq!(recall.query.as_str(), "matcha");
        assert_eq!(recall.limit, 20);
        assert_eq!(
            recall.fact_kind.as_ref().map(TrimmedText::as_str),
            Some("preference")
        );

        let raw_statement = "  User prefers matcha over coffee.  \n";
        let remember = IdentityRememberCommand::try_from(IdentityRememberInput {
            fact_kind: Some(" Preference ".to_string()),
            subject: Some(" beverage ".to_string()),
            statement: Some(raw_statement.to_string()),
            attributes: Some(CompatibleObject(json!({"category": "drink"}))),
            aliases: Some(vec!["  tea  ".to_string(), " \n".to_string()]),
            source: Some(" user_direct ".to_string()),
            confidence: Some(0.7),
            reason: Some("  operator stated it  ".to_string()),
            user_id: Some(" user-a ".to_string()),
        })
        .expect("remember command");
        assert!(matches!(remember.fact_kind, CognitiveFactKind::Preference));
        assert_eq!(remember.fact_kind_label.as_str(), "Preference");
        assert_eq!(remember.subject.as_str(), "beverage");
        assert_eq!(remember.statement.as_str(), raw_statement);
        assert_eq!(remember.aliases.len(), 1);
        assert_eq!(remember.aliases[0].as_str(), "tea");
        assert_eq!(
            remember.reason.as_ref().map(RequiredContent::as_str),
            Some("  operator stated it  ")
        );
    }

    #[test]
    fn identity_remember_command_rejects_blank_statement() {
        let error = IdentityRememberCommand::try_from(IdentityRememberInput {
            fact_kind: Some("note".to_string()),
            subject: Some("topic".to_string()),
            statement: Some(" \n\t".to_string()),
            attributes: None,
            aliases: None,
            source: None,
            confidence: None,
            reason: None,
            user_id: None,
        })
        .expect_err("blank statement should fail");
        assert!(error.to_string().contains("statement is required"));
    }

    #[test]
    fn identity_context_command_normalizes_ids_and_mode_bounds() {
        let command = IdentityContextCommand::try_from(IdentityContextInput {
            user_id: Some(" user-a ".to_string()).into(),
            persona_id: Some(" persona-a ".to_string()).into(),
            channel_id: Some(" channel-a ".to_string()).into(),
            relationship_limit: Some(999).into(),
            mode: Some(" POLICY ".to_string()).into(),
        })
        .expect("context command");
        assert_eq!(
            command.user_id.as_ref().map(TrimmedText::as_str),
            Some("user-a")
        );
        assert_eq!(
            command.persona_id.as_ref().map(TrimmedText::as_str),
            Some("persona-a")
        );
        assert_eq!(command.relationship_limit, 64);
        assert!(matches!(command.mode, IdentityContextMode::Policy));
    }

    #[test]
    fn identity_context_command_rejects_unknown_mode() {
        let error = IdentityContextCommand::try_from(IdentityContextInput {
            user_id: None::<String>.into(),
            persona_id: None::<String>.into(),
            channel_id: None::<String>.into(),
            relationship_limit: None::<usize>.into(),
            mode: Some("unknown".to_string()).into(),
        })
        .expect_err("unknown identity context mode should fail");
        assert!(
            error
                .to_string()
                .contains("unsupported identity context mode")
        );
    }
}
