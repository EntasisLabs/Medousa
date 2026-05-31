use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::session::{ConversationTurn, SessionHistorySummary};
use crate::stage_routing::StageRoutingMatrix;

pub const DEFAULT_DAEMON_BIND: &str = "127.0.0.1:7419";
pub const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:7419";

pub fn resolve_daemon_url(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| std::env::var("MEDOUSA_DAEMON_URL").ok())
        .or_else(|| std::env::var("STASIS_DAEMON_URL").ok())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub backend: String,
    pub worker_id: String,
    pub now_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueAskRequest {
    pub prompt: String,
    pub policy_profile: Option<String>,
    pub model_hint: Option<String>,
    pub max_turns: Option<u32>,
    pub identity_user_id: Option<String>,
    pub identity_persona_id: Option<String>,
    pub identity_channel_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueReportRequest {
    pub query: String,
    pub policy_profile: Option<String>,
    pub model_hint: Option<String>,
    pub max_turns: Option<u32>,
    pub identity_user_id: Option<String>,
    pub identity_persona_id: Option<String>,
    pub identity_channel_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueuePromptRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub policy_profile: Option<String>,
    pub model_hint: Option<String>,
    pub identity_user_id: Option<String>,
    pub identity_persona_id: Option<String>,
    pub identity_channel_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityContextRequest {
    pub user_id: Option<String>,
    pub persona_id: Option<String>,
    pub channel_id: Option<String>,
    pub policy_profile: Option<String>,
    pub relationship_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueResponse {
    pub job_id: String,
    pub queue: String,
    pub accepted_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistoryListRequest {
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistoryListResponse {
    pub sessions: Vec<SessionHistorySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistoryResponse {
    pub session_id: String,
    pub turns: Vec<ConversationTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAppendTurnRequest {
    pub turn: ConversationTurn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAppendTurnResponse {
    pub session_id: String,
    pub stored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResultResponse {
    pub job_id: String,
    pub status: String,
    pub is_terminal: bool,
    pub attempt_count: usize,
    pub latest_outcome: Option<String>,
    pub latest_execution_id: Option<String>,
    pub output_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCitationResponse {
    pub source: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEvidenceReportResponse {
    pub session_id: String,
    pub artifact_id: String,
    pub extraction_id: Option<String>,
    pub pack_id: String,
    pub verification_id: Option<String>,
    pub verification_state: String,
    pub confidence_score: f32,
    pub citation_coverage: f32,
    pub supported_claim_ratio: f32,
    pub total_claims: usize,
    pub supported_claims: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobReportResponse {
    pub job_id: String,
    pub status: String,
    pub is_terminal: bool,
    pub attempt_count: usize,
    pub latest_outcome: Option<String>,
    pub latest_execution_id: Option<String>,
    pub output_text: Option<String>,
    pub citations: Vec<JobCitationResponse>,
    pub evidence_report: Option<JobEvidenceReportResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRecurringPromptRequest {
    pub id: Option<String>,
    pub queue: Option<String>,
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub cron_expr: String,
    pub timezone: Option<String>,
    pub jitter_seconds: Option<i64>,
    pub enabled: Option<bool>,
    pub max_attempts: Option<u32>,
    pub policy_profile: Option<String>,
    pub model_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRecurringResponse {
    pub recurring_id: String,
    pub queue: String,
    pub next_run_at_utc: DateTime<Utc>,
    pub cron_expr: String,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatsResponse {
    pub enqueued_jobs: usize,
    pub running_jobs: usize,
    pub succeeded_jobs: usize,
    pub failed_jobs: usize,
    pub dead_letter_jobs: usize,
    pub pending_outbox_events: usize,
    pub recurring_definitions: usize,
    pub last_tick_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatPolicyResponse {
    pub min_significance: f32,
    pub dead_letter_weight: f32,
    pub failed_weight: f32,
    pub outbox_weight: f32,
    pub activity_weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatDeliveryPolicyResponse {
    pub min_notify_interval_secs: u64,
    pub quiet_hours_start_utc: Option<u8>,
    pub quiet_hours_end_utc: Option<u8>,
    pub in_quiet_hours: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatDeliveryMetricsResponse {
    pub tick_evaluations: u64,
    pub notify_decisions: u64,
    pub dispatched_notifications: u64,
    pub suppressed_quiet_hours: u64,
    pub suppressed_min_interval: u64,
    pub last_notify_decision_at_utc: Option<DateTime<Utc>>,
    pub last_dispatched_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatStatusResponse {
    pub lane: String,
    pub lane_policy_profile: String,
    pub action: String,
    pub significance: f32,
    pub reason: String,
    pub policy: HeartbeatPolicyResponse,
    pub delivery_policy: HeartbeatDeliveryPolicyResponse,
    pub delivery_metrics: HeartbeatDeliveryMetricsResponse,
    pub materialized_jobs: usize,
    pub processed_job: bool,
    pub published_events: usize,
    pub failed_jobs: usize,
    pub dead_letter_jobs: usize,
    pub pending_outbox_events: usize,
    pub last_tick_at_utc: Option<DateTime<Utc>>,
    pub now_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactVerificationPolicyInput {
    pub min_citation_coverage: f32,
    pub min_avg_support_strength: f32,
    pub min_supported_claim_ratio: f32,
    pub min_claim_support_strength: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ArtifactCommandSpec {
    Lookup { query: Option<String> },
    Chunks { query: Option<String> },
    List { limit: usize },
    Maintain {
        max_per_session: usize,
        max_age_days: i64,
    },
    Extract { query: Option<String> },
    Extractions { limit: usize },
    Pack {
        artifact_query: String,
        max_tokens: usize,
        max_claims: usize,
        max_chunks: usize,
    },
    Packs { limit: usize },
    PackUse { query: Option<String> },
    PackAuto,
    Verify { query: Option<String> },
    Verifications { limit: usize },
    Verification { query: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactCommandRequest {
    pub session_id: String,
    pub selected_context_pack_query: Option<String>,
    pub command: ArtifactCommandSpec,
    pub verification_policy: Option<ArtifactVerificationPolicyInput>,
    pub verifier_route_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactCommandResponse {
    pub selected_context_pack_query: Option<String>,
    pub rendered_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum StageRouteCommandSpec {
    Routes {
        role: Option<String>,
    },
    Set {
        role: String,
        target: String,
        policy_profile: Option<String>,
        fallback_chain: Option<Vec<String>>,
    },
    Reset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRouteCommandRequest {
    pub stage_routing: StageRoutingMatrix,
    pub provider: String,
    pub model: String,
    pub command: StageRouteCommandSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRouteCommandResponse {
    pub stage_routing: StageRoutingMatrix,
    pub rendered_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeVerifyPolicyState {
    pub min_citation_coverage: String,
    pub min_avg_support_strength: String,
    pub min_supported_claim_ratio: String,
    pub min_claim_support_strength: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum RuntimeConfigCommandSpec {
    Model { args: Vec<String> },
    Depth { mode: Option<String> },
    VerifyPolicy {
        args: Vec<String>,
        current: RuntimeVerifyPolicyState,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfigCommandRequest {
    pub current_provider: String,
    pub current_model: String,
    pub draft_provider: String,
    pub draft_model: String,
    pub current_response_depth_mode: String,
    pub command: RuntimeConfigCommandSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfigCommandResponse {
    pub rendered_output: Option<String>,
    pub next_draft_provider: String,
    pub next_draft_model: String,
    pub next_response_depth_mode: String,
    pub next_verify_policy_draft: Option<RuntimeVerifyPolicyState>,
    pub should_apply_settings: bool,
    pub should_persist_depth_defaults: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveTurnRequest {
    pub session_id: String,
    pub prompt: String,
    pub persist_user_turn: bool,
    pub response_depth_mode: String,
    pub provider: String,
    pub model: String,
    pub stage_routing: StageRoutingMatrix,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveTurnResponse {
    pub turn_id: String,
    pub accepted_at_utc: DateTime<Utc>,
    pub stream_url: String,
    pub stream_ready: bool,
    pub fallback_to_local: bool,
    pub fallback_reason: Option<String>,
    pub daemon_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveTurnStreamEvent {
    pub turn_id: String,
    pub event_type: String,
    pub phase: String,
    pub message: String,
    pub content_delta: Option<String>,
    pub reasoning_delta: Option<String>,
    pub final_text: Option<String>,
    pub tool_names: Option<Vec<String>>,
    pub terminal: bool,
    pub emitted_at_utc: DateTime<Utc>,
}

// ── Ingester types ────────────────────────────────────────────────────────────

/// Request from any channel adapter to the centralized ingester.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestRequest {
    /// Channel type identifier, e.g. "telegram", "discord", "cli"
    pub channel: String,
    /// User identifier within the channel, e.g. "telegram:user:12345"
    pub user_id: String,
    /// Channel/chat/conversation identifier, e.g. "telegram:chat:67890"
    pub channel_id: String,
    /// The text content of the message (command or prompt)
    pub text: String,
}

/// Response from the centralized ingester.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestResponse {
    /// The resolved or created session_id for this channel+user pair
    pub session_id: String,
    /// If a job was enqueued, its id (for polling/streaming)
    pub job_id: Option<String>,
    /// Immediate text reply (help text, confirmation, error message)
    pub reply: String,
    /// Whether this is a brand-new session (first message or after /new)
    pub is_new_session: bool,
    /// SSE stream id when a job-backed ask is processing
    #[serde(default)]
    pub stream_id: Option<String>,
    /// Absolute URL for SSE stream consumption by adapters
    #[serde(default)]
    pub stream_url: Option<String>,
    /// Whether the stream endpoint is ready for subscription
    #[serde(default)]
    pub stream_ready: bool,
}
