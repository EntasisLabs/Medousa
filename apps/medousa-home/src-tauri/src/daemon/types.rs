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
    pub provider: String,
    pub model: String,
    pub stage_routing: StageRoutingMatrix,
    #[serde(default)]
    pub surface: Option<TurnSurfaceContext>,
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

#[derive(Debug, Clone, Serialize)]
pub struct DaemonHealth {
    pub ok: bool,
    pub message: String,
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
    pub associations: WorkCardAssociations,
}
