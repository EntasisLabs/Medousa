use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::session::{ConversationTurn, SessionHistorySummary};
use crate::stage_routing::StageRoutingMatrix;

pub const DEFAULT_DAEMON_BIND: &str = "127.0.0.1:7419";
pub const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:7419";
pub const DEFAULT_DAEMON_PORT: u16 = 7419;

pub fn parse_daemon_bind_port(bind: &str) -> u16 {
    bind
        .rsplit(':')
        .next()
        .and_then(|port| port.parse().ok())
        .unwrap_or(DEFAULT_DAEMON_PORT)
}

/// Best-effort LAN IPv4 for advertising a daemon bound to `0.0.0.0`.
pub fn detect_lan_ipv4() -> Option<String> {
    use std::net::{IpAddr, UdpSocket};
    let socket = UdpSocket::bind(format!("0.0.0.0:0")).ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(addr) if !addr.is_loopback() && !addr.is_unspecified() => Some(addr.to_string()),
        _ => None,
    }
}

/// Bind address when `--public` is passed to `medousa start daemon`.
pub fn resolve_public_daemon_bind(explicit_bind: Option<&str>, fallback_port: u16) -> String {
    let port = explicit_bind
        .map(parse_daemon_bind_port)
        .unwrap_or(fallback_port);
    format!("0.0.0.0:{port}")
}

/// URL phones and other LAN clients should use (when binding publicly).
pub fn resolve_mobile_client_daemon_url(bind: &str) -> Option<String> {
    let port = parse_daemon_bind_port(bind);
    detect_lan_ipv4().map(|host| format!("http://{host}:{port}"))
}

/// Local URL for health checks when the daemon binds to all interfaces.
pub fn resolve_local_daemon_health_url(bind: &str) -> String {
    format!("http://127.0.0.1:{}", parse_daemon_bind_port(bind))
}

pub fn resolve_daemon_url(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| std::env::var("MEDOUSA_DAEMON_URL").ok())
        .or_else(|| std::env::var("STASIS_DAEMON_URL").ok())
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string())
}

/// Client-reachable base URL embedded in stream links (distinct from `--bind`).
pub fn resolve_daemon_public_base_url(bind: &str) -> String {
    if let Ok(public) = std::env::var("MEDOUSA_DAEMON_PUBLIC_URL") {
        let trimmed = public.trim().trim_end_matches('/');
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let bind = bind.trim();
    if bind.starts_with("0.0.0.0:") || bind.starts_with("[::]:") {
        if let Some(host) = detect_lan_ipv4() {
            let port = parse_daemon_bind_port(bind);
            return format!("http://{host}:{port}");
        }
        if let Ok(host) = std::env::var("MEDOUSA_DEV_HOST") {
            let host = host.trim();
            if !host.is_empty() {
                let port = parse_daemon_bind_port(bind);
                return format!("http://{host}:{port}");
            }
        }
    }

    format!("http://{bind}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub backend: String,
    pub worker_id: String,
    pub now_utc: DateTime<Utc>,
    #[serde(default = "default_agent_runtime_version")]
    pub agent_runtime_version: String,
    #[serde(default)]
    pub tool_registry_count: usize,
    #[serde(default)]
    pub last_agent_turn_latency_ms: Option<u64>,
    #[serde(default)]
    pub last_agent_turn_at_utc: Option<DateTime<Utc>>,
    /// Active workshop identity profile (`user:{slug}`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_profile_id: String,
    /// Human label for `active_profile_id`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_profile_display_name: String,
}

fn default_agent_runtime_version() -> String {
    crate::agent_runtime::AGENT_RUNTIME_VERSION.to_string()
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
    /// Primary YAML manuscript specialty for the ask turn.
    #[serde(default)]
    pub manuscript_id: Option<String>,
    /// Extra manuscript specialties beyond `manuscript_id`.
    #[serde(default)]
    pub additional_manuscript_ids: Option<Vec<String>>,
    /// Capability catalog ids the operator suggests for this ask.
    #[serde(default)]
    pub suggested_capability_ids: Option<Vec<String>>,
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
    /// `full` | `policy` | `cognitive` (default: full for operator inspect)
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfileRecordDto {
    pub profile_id: String,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
    pub is_default: bool,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListUserProfilesResponse {
    pub profiles: Vec<UserProfileRecordDto>,
    pub active_profile_id: String,
    pub resolved_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserProfileRequest {
    /// Short slug (`work`, `home`) — stored as `user:{slug}`.
    pub slug: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserProfileResponse {
    pub profile: UserProfileRecordDto,
    pub active_profile_id: String,
    pub resolved_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetActiveUserProfileRequest {
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetActiveUserProfileResponse {
    pub active_profile_id: String,
    pub resolved_user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportUserProfileRequest {
    pub profile_id: String,
    #[serde(default = "default_profile_export_session_limit")]
    pub session_limit: usize,
    #[serde(default = "default_profile_export_node_limit")]
    pub node_limit_per_session: usize,
}

fn default_profile_export_session_limit() -> usize {
    500
}

fn default_profile_export_node_limit() -> usize {
    500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportUserProfileResponse {
    pub bundle: crate::profile_portability::ProfileExportBundle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportUserProfileRequest {
    pub bundle: crate::profile_portability::ProfileExportBundle,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportUserProfileResponse {
    pub dry_run: bool,
    pub profile_id: String,
    pub created_profile: bool,
    pub identity_user_imported: bool,
    pub contacts_imported: usize,
    pub relationships_imported: usize,
    pub locus_nodes_imported: usize,
    pub locus_sessions_touched: usize,
    pub message: String,
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
    /// When `false`, omit verification trust fields from each session row (smaller payloads).
    pub include_verification: Option<bool>,
    /// Case-insensitive substring match on display name, preview, or session id.
    pub q: Option<String>,
    /// Opaque pagination cursor from a prior response (`next_cursor`).
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistoryListResponse {
    pub sessions: Vec<SessionHistorySummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
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
pub struct SessionSetDisplayNameRequest {
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSetDisplayNameResponse {
    pub session_id: String,
    pub display_name: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interim_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskJobCompleteActionsRequest {
    #[serde(default)]
    pub write_journal_path: Option<String>,
    #[serde(default)]
    pub notify_channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskJobCompleteActionsResponse {
    pub job_id: String,
    pub ok: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub journal_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notified_channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveAskJobRequest {
    #[serde(default)]
    pub purge_output: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveAskJobResponse {
    pub job_id: String,
    pub archived: bool,
    pub message: String,
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
    /// Optional channel push target for each successful materialized run.
    pub delivery: Option<serde_json::Value>,
    /// Medousa session id for `delivery.mode=linked_channel` (defaults to `recurring-{id}`).
    pub session_id: Option<String>,
    /// `prompt` (single LLM, default) or `agent_turn` (full Medousa tool loop per tick).
    pub execution_mode: Option<String>,
    /// Optional YAML identity manuscript (loads task template, tool allowlist, pins).
    #[serde(default)]
    pub manuscript_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRecurringResponse {
    pub recurring_id: String,
    pub queue: String,
    pub next_run_at_utc: DateTime<Utc>,
    pub cron_expr: String,
    pub timezone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RecurringListQuery {
    #[serde(default)]
    pub enabled_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringDefinitionEntry {
    pub recurring_id: String,
    pub queue: String,
    pub job_type: String,
    pub cron_expr: String,
    pub timezone: String,
    pub enabled: bool,
    pub next_run_at_utc: DateTime<Utc>,
    pub last_run_at_utc: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manuscript_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringListResponse {
    pub count: usize,
    pub recurring: Vec<RecurringDefinitionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateRecurringRequest {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub cron_expr: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRecurringResponse {
    pub recurring_id: String,
    pub enabled: bool,
    pub cron_expr: String,
    pub timezone: String,
    pub next_run_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteRecurringResponse {
    pub recurring_id: String,
    pub deleted: bool,
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

/// Live workshop runtime defaults from the daemon host (`tui_defaults.json` + env).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDefaultsResponse {
    pub backend: String,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    pub reasoning_effort: String,
    pub base_url: Option<String>,
    pub stage_routing: StageRoutingMatrix,
    pub work_card_hide_after_hours: u32,
    pub work_card_wipe_after_days: u32,
    /// Active workshop identity profile (`user:{slug}`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_profile_id: String,
    /// Human label for `active_profile_id`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub active_profile_display_name: String,
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
    Reasoning { mode: Option<String> },
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
    #[serde(default)]
    pub current_reasoning_effort: String,
    pub command: RuntimeConfigCommandSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfigCommandResponse {
    pub rendered_output: Option<String>,
    pub next_draft_provider: String,
    pub next_draft_model: String,
    pub next_response_depth_mode: String,
    pub next_reasoning_effort: String,
    pub next_verify_policy_draft: Option<RuntimeVerifyPolicyState>,
    pub should_apply_settings: bool,
    pub should_persist_depth_defaults: bool,
    pub should_persist_reasoning_defaults: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct TurnSurfaceContext {
    /// Adapter surface: telegram, discord, slack, home-desktop, home-ios, tui, api, …
    #[serde(default)]
    pub channel_surface: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

impl TurnSurfaceContext {
    pub fn from_ingest(channel: &str, channel_id: &str, user_id: &str) -> Self {
        Self {
            channel_surface: Some(channel.trim().to_string()),
            channel_id: Some(channel_id.trim().to_string()),
            user_id: Some(user_id.trim().to_string()),
        }
    }

    pub fn tui() -> Self {
        Self {
            channel_surface: Some("tui".to_string()),
            channel_id: None,
            user_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveTurnRequest {
    pub session_id: String,
    pub prompt: String,
    pub persist_user_turn: bool,
    pub response_depth_mode: String,
    #[serde(default)]
    pub reasoning_effort: String,
    pub provider: String,
    pub model: String,
    pub stage_routing: StageRoutingMatrix,
    /// Channel adapter context for ambient prompting (ingest/TUI surfaces).
    #[serde(default)]
    pub surface: Option<TurnSurfaceContext>,
    /// When set, overrides `tui_defaults.json` for this turn (TUI live settings).
    #[serde(default)]
    pub max_tool_rounds: Option<usize>,
    #[serde(default)]
    pub retry_runtime_max_rounds: Option<usize>,
    /// YAML manuscript specialty for ranked digest + scheduled tool allowlist.
    #[serde(default)]
    pub manuscript_id: Option<String>,
    #[serde(default)]
    pub additional_manuscript_ids: Option<Vec<String>>,
    #[serde(default)]
    pub suggested_capability_ids: Option<Vec<String>>,
    /// Composer voice stance — short appendix block (not a manuscript specialty).
    #[serde(default)]
    pub voice_preset_id: Option<String>,
    #[serde(default)]
    pub voice_appendix: Option<String>,
    #[serde(default)]
    pub scheduled_tool_allowlist: Option<Vec<String>>,
    /// User media uploaded to local medousa/media/ before this turn (P5a).
    #[serde(default)]
    pub media_refs: Vec<MediaRef>,
    /// Optional identity principal override (debug/internal). Default: active workshop profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_user_id: Option<String>,
}

/// Reference to a user file stored locally under medousa/media/ (not inline bytes).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaRef {
    pub media_id: String,
    /// image | document | spreadsheet | audio
    pub kind: String,
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaUploadResponse {
    pub media_id: String,
    pub mime: String,
    pub byte_size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// True when local text extraction succeeded at upload (P5a-text).
    #[serde(default)]
    pub text_extracted: bool,
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

/// Unified turn ticket — interactive chat or background `/ask` on the same SSE contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTurnTicketRequest {
    pub session_id: String,
    pub prompt: String,
    #[serde(default)]
    pub mode: TurnTicketMode,
    #[serde(default = "default_persist_user_turn")]
    pub persist_user_turn: bool,
    #[serde(default = "default_response_depth_mode")]
    pub response_depth_mode: String,
    #[serde(default)]
    pub reasoning_effort: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub stage_routing: Option<StageRoutingMatrix>,
    #[serde(default)]
    pub surface: Option<TurnSurfaceContext>,
    #[serde(default)]
    pub model_hint: Option<String>,
    #[serde(default)]
    pub manuscript_id: Option<String>,
    #[serde(default)]
    pub additional_manuscript_ids: Option<Vec<String>>,
    #[serde(default)]
    pub suggested_capability_ids: Option<Vec<String>>,
    /// Composer voice stance — short appendix block (not a manuscript specialty).
    #[serde(default)]
    pub voice_preset_id: Option<String>,
    #[serde(default)]
    pub voice_appendix: Option<String>,
    /// User media uploaded to local medousa/media/ before this turn (P5a).
    #[serde(default)]
    pub media_refs: Vec<MediaRef>,
    /// Optional identity principal override (debug/internal). Default: active workshop profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_user_id: Option<String>,
}

fn default_persist_user_turn() -> bool {
    true
}

fn default_response_depth_mode() -> String {
    "standard".to_string()
}

pub use crate::turn_ticket::{TurnTicketMode, TurnTicketPhase};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnTicketResponse {
    pub turn_id: String,
    pub session_id: String,
    pub mode: TurnTicketMode,
    pub phase: TurnTicketPhase,
    pub accepted_at_utc: DateTime<Utc>,
    pub stream_url: String,
    pub stream_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_card_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionActiveTurnsResponse {
    pub session_id: String,
    pub turns: Vec<TurnTicketRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnTicketRecord {
    pub turn_id: String,
    pub session_id: String,
    pub mode: TurnTicketMode,
    pub phase: TurnTicketPhase,
    pub stream_url: String,
    pub prompt_preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_card_id: Option<String>,
    pub composer_handoff: bool,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    /// Turn budget approval pause — card id for Home deep link / notifications.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_rounds: Option<usize>,
    /// Turn worker handoff — workspace card id (`work-…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    /// Structured tool bus (P1) — correlates started/finished pair.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_input_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_output_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_round: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_artifact_refs: Option<Vec<StreamToolArtifactRef>>,
    /// Human-facing status whisper for rich surfaces (Home default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_message: Option<String>,
    /// Engine/TUI telemetry — shown only when the operator opts into engine details.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamToolArtifactRef {
    pub role: String,
    pub content_type: String,
    pub byte_size: usize,
    pub hash64: String,
}

// ── Ingester types ────────────────────────────────────────────────────────────

/// Optional attachment forwarded by a channel adapter.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IngestAttachment {
    pub kind: String,
    pub content: String,
}

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
    /// Optional attachment payloads merged into ask prompts
    #[serde(default)]
    pub attachments: Vec<IngestAttachment>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliverPollResponse {
    pub job_id: String,
    /// pending | delivered | failed | not_registered
    pub status: String,
    pub delivered_at_utc: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryHealthResponse {
    pub endpoint_id: String,
    pub endpoint_seeded: bool,
    pub endpoint_target: String,
    pub deliver_webhook_auth_configured: bool,
    pub pending_job_deliveries: usize,
    pub last_delivery_at_utc: Option<DateTime<Utc>>,
    pub last_delivery_latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuationStatusResponse {
    pub pending_count: usize,
    pub consumed_count: usize,
    pub resumed_count: usize,
    pub dead_letter_pending_count: usize,
    pub total_count: usize,
    pub last_resume_at_utc: Option<DateTime<Utc>>,
    pub last_resume_child_job_id: Option<String>,
    pub last_resume_turn_correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnContinuationLineageEntry {
    pub child_job_id: String,
    pub turn_correlation_id: String,
    pub session_id: String,
    pub tool_name: String,
    pub job_type: String,
    pub await_mode: String,
    pub status: String,
    pub turn_finished: bool,
    pub turn_outcome: Option<String>,
    pub child_was_dead_letter: bool,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnContinuationLineageResponse {
    pub turn_correlation_id: String,
    pub records: Vec<TurnContinuationLineageEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayAndResumeResponse {
    pub job_id: String,
    pub replayed: bool,
    pub job_succeeded: bool,
    pub agent_turn_resumed: bool,
    pub message: String,
}

// ── Turn budget requests (tool-round extensions) ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnBudgetRequestListQuery {
    pub limit: Option<usize>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnBudgetRequestRecord {
    pub request_id: String,
    pub turn_correlation_id: Option<String>,
    pub stream_turn_id: u64,
    pub session_id: String,
    pub channel: Option<String>,
    pub rounds_executed: usize,
    pub max_tool_rounds: usize,
    pub requested_rounds: usize,
    pub granted_rounds: Option<usize>,
    pub reason: String,
    pub progress_summary: Option<String>,
    pub status: String,
    pub resolved_by: Option<String>,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub resolved_at_utc: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnBudgetRequestListResponse {
    pub requests: Vec<TurnBudgetRequestRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnBudgetApproveRequest {
    pub extra_rounds: Option<usize>,
    pub resolved_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnBudgetDenyRequest {
    pub resolved_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnBudgetRequestResponse {
    pub request: TurnBudgetRequestRecord,
    pub message: String,
}

// ── Workspace (Medousa Home — Phase W1) ─────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkBoardColumn {
    Backlog,
    InFlight,
    WrappingUp,
    Done,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkCardId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkCard {
    pub id: WorkCardId,
    pub column: WorkBoardColumn,
    pub title: String,
    pub status_label: String,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkCardKind {
    StasisJob,
    TurnWorker,
    InteractiveTurn,
    AskJob,
    RecurringTick,
    TurnBudgetRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkCardAssociations {
    #[serde(default)]
    pub vault_paths: Vec<String>,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
    #[serde(default)]
    pub locus_node_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkCardDetail {
    pub card: WorkCard,
    pub kind: WorkCardKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manuscript_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_ack: Option<String>,
    #[serde(default)]
    pub wrapping_up_reasons: Vec<String>,
    pub terminal: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_names: Option<Vec<String>>,
    #[serde(default)]
    pub associations: WorkCardAssociations,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceEventKind {
    JobEnqueued,
    JobStarted,
    JobSucceeded,
    JobFailed,
    WorkDelegated,
    WorkCompleted,
    WorkWrappingUp,
    WorkUnblocked,
    TurnAccepted,
    TurnCompleted,
    AgentReplied,
    VaultNoteCreated,
    VaultNoteUpdated,
    IdentityRemembered,
    LocusBridgeWritten,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceEventActor {
    System,
    Agent,
    Operator,
    Scheduler,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEventRef {
    pub ref_type: String,
    pub ref_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceEvent {
    pub id: String,
    pub timestamp_utc: DateTime<Utc>,
    pub kind: WorkspaceEventKind,
    pub actor: WorkspaceEventActor,
    pub summary: String,
    #[serde(default)]
    pub refs: Vec<WorkspaceEventRef>,
    /// Operator-facing task label (full prompt when user_ack is a slug).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail_line: Option<String>,
    /// Secondary context: intent, tools, wrapping-up reasons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_line: Option<String>,
    /// Worker intent or job family at emit time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// Tools invoked on the work card when the event was emitted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub workspace_revision: u64,
    pub server_time_utc: DateTime<Utc>,
    pub cards: Vec<WorkCard>,
    pub counts_by_column: std::collections::HashMap<String, u32>,
    pub feed_tail: Vec<WorkspaceEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceCardsQuery {
    pub session_id: Option<String>,
    pub column: Option<String>,
    pub limit: Option<usize>,
    #[serde(default)]
    pub include_terminal: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCardsResponse {
    pub workspace_revision: u64,
    pub cards: Vec<WorkCard>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceFeedQuery {
    pub since_id: Option<String>,
    pub since_revision: Option<u64>,
    pub limit: Option<usize>,
    pub card_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceFeedResponse {
    pub workspace_revision: u64,
    pub events: Vec<WorkspaceEvent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceSnapshotQuery {
    pub since_revision: Option<u64>,
    pub feed_tail_limit: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceStreamQuery {
    pub since_revision: Option<u64>,
    pub session_id: Option<String>,
    pub feed_tail_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceStreamEvent {
    pub workspace_revision: u64,
    pub stream_event_type: String,
    pub emitted_at_utc: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<WorkCard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_event: Option<WorkspaceEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counts: Option<std::collections::HashMap<String, u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<WorkspaceSnapshot>,
}

/// Frozen workspace HTTP contract version (Phase W3 gate).
pub const WORKSPACE_API_VERSION: &str = "workspace-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceLinkVaultRequest {
    pub vault_path: String,
}

// ── Vault (Medousa Home — Phase V0) ───────────────────────────────────────────

/// Frozen vault HTTP contract version (Phase V0 gate).
pub const VAULT_API_VERSION: &str = "vault-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultNote {
    pub path: String,
    pub title: String,
    pub byte_size: usize,
    pub content_hash: String,
    pub modified_at_utc: DateTime<Utc>,
    pub created_at_utc: DateTime<Utc>,
    pub tags: Vec<String>,
    pub wikilinks_out: Vec<String>,
    pub backlinks: Vec<String>,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultNoteSummary {
    pub path: String,
    pub title: String,
    pub modified_at_utc: DateTime<Utc>,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultNotesListResponse {
    pub notes: Vec<VaultNote>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultNotesQuery {
    pub prefix: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultNoteContentResponse {
    pub note: VaultNote,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultWriteRequest {
    #[serde(default)]
    pub path: Option<String>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultWriteResponse {
    pub note: VaultNote,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultDeleteResponse {
    pub path: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSearchHit {
    pub note: VaultNoteSummary,
    pub score: f32,
    pub matched_terms: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSearchResponse {
    pub query: String,
    pub hits: Vec<VaultSearchHit>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultSearchQuery {
    pub q: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultBacklinksQuery {
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultBacklinksResponse {
    pub path: String,
    pub backlinks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCardActionResponse {
    pub workspace_revision: u64,
    pub card_id: String,
    pub action: String,
    pub ok: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replayed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_succeeded: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub associations: Option<WorkCardAssociations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRebuildResponse {
    pub workspace_revision: u64,
    pub card_count: usize,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManuscriptCatalogQuery {
    pub prefix: Option<String>,
    pub limit: Option<usize>,
    #[serde(default)]
    pub skills_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManuscriptScriptEntry {
    pub relative_path: String,
    pub risk_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManuscriptCatalogEntry {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub scope: String,
    pub path: String,
    pub has_scripts: bool,
    #[serde(default)]
    pub scripts: Vec<ManuscriptScriptEntry>,
    pub openshell_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManuscriptCatalogResponse {
    pub count: usize,
    pub manuscripts: Vec<ManuscriptCatalogEntry>,
}

// ── Locus / STTP (read-only context view) ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocusNodeSummary {
    pub sync_key: String,
    pub session_id: String,
    pub tier: String,
    pub timestamp: DateTime<Utc>,
    pub context_summary: String,
    pub psi: f64,
    pub rho: f64,
    pub kappa: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_avec: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_avec: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocusNodesListResponse {
    pub retrieved: usize,
    pub nodes: Vec<LocusNodeSummary>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocusNodesQuery {
    pub session_id: Option<String>,
    pub limit: Option<usize>,
    pub q: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocusNodeDetailResponse {
    pub node: LocusNodeSummary,
    pub raw: String,
}
