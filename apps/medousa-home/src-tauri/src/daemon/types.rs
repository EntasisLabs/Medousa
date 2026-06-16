use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:7419";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkCard {
    pub id: String,
    pub column: String,
    pub title: String,
    pub status_label: String,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEventRef {
    pub ref_type: String,
    pub ref_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEvent {
    pub id: String,
    pub timestamp_utc: DateTime<Utc>,
    pub kind: String,
    pub actor: String,
    pub summary: String,
    #[serde(default)]
    pub refs: Vec<WorkspaceEventRef>,
    #[serde(default)]
    pub detail_line: Option<String>,
    #[serde(default)]
    pub context_line: Option<String>,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSnapshot {
    pub workspace_revision: u64,
    pub server_time_utc: DateTime<Utc>,
    pub cards: Vec<WorkCard>,
    pub counts_by_column: HashMap<String, u32>,
    pub feed_tail: Vec<WorkspaceEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceStreamEvent {
    pub workspace_revision: u64,
    pub stream_event_type: String,
    pub emitted_at_utc: DateTime<Utc>,
    #[serde(default)]
    pub card: Option<WorkCard>,
    #[serde(default)]
    pub feed_event: Option<WorkspaceEvent>,
    #[serde(default)]
    pub counts: Option<HashMap<String, u32>>,
    #[serde(default)]
    pub snapshot: Option<WorkspaceSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRoute {
    pub role: String,
    pub provider: String,
    pub model: String,
    pub policy_profile: String,
    pub fallback_chain: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageRoutingMatrix {
    pub orchestrator: StageRoute,
    pub chunker: StageRoute,
    pub extractor: StageRoute,
    pub summarizer: StageRoute,
    pub verifier: StageRoute,
    pub packer: StageRoute,
    pub final_response: StageRoute,
}

impl StageRoutingMatrix {
    pub fn default_for(provider: &str, model: &str) -> Self {
        let policy = "balanced".to_string();
        Self {
            orchestrator: route("orchestrator", provider, model, "orchestrator", &policy),
            chunker: route("chunker", provider, model, "chunker", "fast"),
            extractor: route("extractor", provider, model, "extractor", "analytical"),
            summarizer: route("summarizer", provider, model, "summarizer", "balanced"),
            verifier: route("verifier", provider, model, "verifier", "strict"),
            packer: route("packer", provider, model, "packer", "balanced"),
            final_response: route("final_response", provider, model, "final_response", "balanced"),
        }
    }
}

fn route(role: &str, provider: &str, model: &str, fallback: &str, policy: &str) -> StageRoute {
    StageRoute {
        role: role.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        policy_profile: policy.to_string(),
        fallback_chain: vec![fallback.to_string(), "safe-default".to_string()],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TurnSurfaceContext {
    #[serde(default)]
    pub channel_surface: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
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
    #[serde(default)]
    pub surface: Option<TurnSurfaceContext>,
    #[serde(default)]
    pub media_refs: Vec<MediaRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaRef {
    pub media_id: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_rounds: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_message: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
pub struct DaemonHealth {
    pub ok: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_registry_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveTurnAccepted {
    pub turn_id: String,
    pub stream_url: String,
}

// ── Vault ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultNotesListResponse {
    pub notes: Vec<VaultNote>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultNoteContentResponse {
    pub note: VaultNote,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultWriteResponse {
    pub note: VaultNote,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSearchHit {
    pub note: VaultNoteSummary,
    pub score: f32,
    pub matched_terms: Vec<String>,
    #[serde(default)]
    pub snippet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultNoteSummary {
    pub path: String,
    pub title: String,
    pub modified_at_utc: DateTime<Utc>,
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSearchResponse {
    pub query: String,
    pub hits: Vec<VaultSearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultBacklinksResponse {
    pub path: String,
    pub backlinks: Vec<String>,
}

// ── Workspace card detail ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkCardAssociations {
    #[serde(default)]
    pub vault_paths: Vec<String>,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
    #[serde(default)]
    pub locus_node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceCardActionResponse {
    pub workspace_revision: u64,
    pub card_id: String,
    pub action: String,
    pub ok: bool,
    pub message: String,
    #[serde(default)]
    pub job_id: Option<String>,
    #[serde(default)]
    pub replayed: Option<bool>,
    #[serde(default)]
    pub job_succeeded: Option<bool>,
    #[serde(default)]
    pub associations: Option<WorkCardAssociations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkCardDetail {
    pub card: WorkCard,
    pub kind: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub correlation_id: Option<String>,
    #[serde(default)]
    pub manuscript_id: Option<String>,
    #[serde(default)]
    pub job_id: Option<String>,
    #[serde(default)]
    pub work_id: Option<String>,
    #[serde(default)]
    pub job_type: Option<String>,
    #[serde(default)]
    pub user_ack: Option<String>,
    #[serde(default)]
    pub wrapping_up_reasons: Vec<String>,
    pub terminal: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub result_excerpt: Option<String>,
    #[serde(default)]
    pub task_line: Option<String>,
    #[serde(default)]
    pub tool_names: Option<Vec<String>>,
    #[serde(default)]
    pub associations: WorkCardAssociations,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistorySummary {
    pub session_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub turns: usize,
    pub verification_runs: usize,
    #[serde(default)]
    pub last_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_verification_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_verification_confidence: Option<f32>,
    #[serde(default)]
    pub last_verification_coverage: Option<f32>,
    #[serde(default)]
    pub last_verification_verified: Option<bool>,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistoryListResponse {
    pub sessions: Vec<SessionHistorySummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnArtifactRef {
    pub role: String,
    pub content_type: String,
    pub byte_size: usize,
    pub hash64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnPart {
    Text {
        markdown: String,
    },
    Reasoning {
        markdown: String,
    },
    ToolRun {
        run_id: String,
        tool_name: String,
        status: String,
        input_summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_summary: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        artifact_refs: Vec<TurnArtifactRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_round: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        started_at: Option<DateTime<Utc>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finished_at: Option<DateTime<Utc>>,
    },
    Handoff {
        handoff_kind: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationTurn {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub tool_names: Vec<String>,
    #[serde(default)]
    pub answer_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<TurnPart>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistoryResponse {
    pub session_id: String,
    pub turns: Vec<ConversationTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub backend: String,
    pub worker_id: String,
    pub now_utc: DateTime<Utc>,
    #[serde(default)]
    pub agent_runtime_version: String,
    #[serde(default)]
    pub tool_registry_count: usize,
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
    #[serde(default)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBindingSummary {
    pub source: String,
    pub reference: String,
    pub available: bool,
    #[serde(default)]
    pub effect_class: Option<String>,
    #[serde(default)]
    pub invoke_via: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityListEntry {
    pub id: String,
    pub title: String,
    pub binding_count: usize,
    #[serde(default)]
    pub description: Option<String>,
    pub domain: String,
    pub has_grapheme: bool,
    pub has_mcp: bool,
    #[serde(default)]
    pub bindings_summary: Vec<CapabilityBindingSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityListResponse {
    pub capabilities: Vec<CapabilityListEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBinding {
    pub source: String,
    pub reference: String,
    pub priority: u16,
    pub available: bool,
    #[serde(default)]
    pub unavailable_reason: Option<String>,
    #[serde(default)]
    pub invoke_via: Option<String>,
    #[serde(default)]
    pub effect_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityImplementations {
    #[serde(default)]
    pub grapheme: Vec<CapabilityBinding>,
    #[serde(default)]
    pub mcp: Vec<CapabilityBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityRecommendation {
    pub source: String,
    pub reference: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityResolveResponse {
    pub capability: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub implementations: CapabilityImplementations,
    #[serde(default)]
    pub recommended: Option<CapabilityRecommendation>,
    #[serde(default)]
    pub gateway_unreachable: Option<bool>,
}

// ── Runtime observability & controls ──────────────────────────────────────────

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
#[serde(tag = "command", rename_all = "snake_case")]
pub enum RuntimeConfigCommandSpec {
    Model { args: Vec<String> },
    Depth { mode: Option<String> },
    Reasoning { mode: Option<String> },
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
    pub should_apply_settings: bool,
    pub should_persist_depth_defaults: bool,
    pub should_persist_reasoning_defaults: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum StageRouteCommandSpec {
    Routes { role: Option<String> },
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

// ── Jobs ────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResultResponse {
    pub job_id: String,
    pub status: String,
    pub is_terminal: bool,
    pub attempt_count: usize,
    pub latest_outcome: Option<String>,
    pub latest_execution_id: Option<String>,
    pub output_text: Option<String>,
    #[serde(default)]
    pub interim_text: Option<String>,
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
    #[serde(default)]
    pub manuscript_id: Option<String>,
    #[serde(default)]
    pub additional_manuscript_ids: Option<Vec<String>>,
    #[serde(default)]
    pub suggested_capability_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueResponse {
    pub job_id: String,
    pub queue: String,
    pub accepted_at_utc: DateTime<Utc>,
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
    #[serde(default)]
    pub journal_path: Option<String>,
    #[serde(default)]
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

// ── Recurring schedules ─────────────────────────────────────────────────────────

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
    #[serde(default)]
    pub manuscript_id: Option<String>,
    #[serde(default)]
    pub prompt_excerpt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecurringListResponse {
    pub count: usize,
    pub recurring: Vec<RecurringDefinitionEntry>,
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
    pub delivery: Option<serde_json::Value>,
    pub session_id: Option<String>,
    pub execution_mode: Option<String>,
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

// ── Identity & artifacts ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityContextRequest {
    pub user_id: Option<String>,
    pub persona_id: Option<String>,
    pub channel_id: Option<String>,
    pub policy_profile: Option<String>,
    pub relationship_limit: Option<usize>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ArtifactCommandSpec {
    Lookup { query: Option<String> },
    List { limit: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactCommandRequest {
    pub session_id: String,
    pub selected_context_pack_query: Option<String>,
    pub command: ArtifactCommandSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactCommandResponse {
    pub selected_context_pack_query: Option<String>,
    pub rendered_output: String,
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
pub struct TurnBudgetRequestResponse {
    pub request: TurnBudgetRequestRecord,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnBudgetRequestListResponse {
    pub requests: Vec<TurnBudgetRequestRecord>,
}
