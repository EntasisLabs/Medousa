//! Application-facing chat contract.
//!
//! Daemon types remain the authority for durable records and execution. These
//! types describe the canonical **client application** timeline projection —
//! identities, ordering, live/persisted reconciliation, and causal links —
//! without depending on any renderer.

use std::collections::BTreeMap;

use medousa_types::{HostTurnContext, MessageReaction, ToolInputParam, TurnStreamEnvelopeV3};
use serde::{Deserialize, Serialize};

#[cfg(feature = "json-schema")]
use schemars::JsonSchema;

/// Monotonic revision for a focused chat session projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(transparent)]
pub struct ChatRevision(pub u64);

impl ChatRevision {
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ChatItemRole {
    User,
    Assistant,
    System,
}

impl ChatItemRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "user" => Self::User,
            "system" => Self::System,
            _ => Self::Assistant,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum ChatLane {
    Chat,
    Ask,
    Worker,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSourceRef {
    pub authority_id: String,
    pub session_id: String,
    pub entry_id: String,
    pub entry_seq: u64,
}

/// Durable coordinate for a committed transcript entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCoordinate {
    pub authority_id: String,
    pub session_id: String,
    pub entry_id: String,
    pub entry_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<TranscriptSourceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ToolArtifactRef {
    pub role: String,
    pub content_type: String,
    pub byte_size: u64,
    pub hash64: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ToolRunState {
    pub run_id: String,
    pub tool_name: String,
    pub status: String,
    pub round: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_params: Option<Vec<ToolInputParam>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_refs: Option<Vec<ToolArtifactRef>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub enum UiArtifactPresentation {
    Inline,
    Panel,
    Fullscreen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct UiArtifact {
    pub artifact_id: String,
    pub mime: String,
    pub label: String,
    pub presentation: UiArtifactPresentation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_px: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_artifact_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatSegment {
    Text {
        #[serde(rename = "segmentId")]
        segment_id: String,
        #[serde(rename = "modelRound")]
        model_round: Option<u64>,
        markdown: String,
        committed: bool,
    },
    Progress {
        #[serde(rename = "progressId")]
        progress_id: String,
        markdown: String,
    },
    ToolGroup {
        #[serde(rename = "groupId")]
        group_id: String,
        #[serde(rename = "toolRound")]
        tool_round: u64,
        runs: Vec<ToolRunState>,
    },
    Artifact {
        artifact: UiArtifact,
    },
    Handoff {
        #[serde(rename = "handoffKind")]
        handoff_kind: String,
        text: String,
        #[serde(rename = "workId", default, skip_serializing_if = "Option::is_none")]
        work_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ChatMediaAttachment {
    pub media_id: String,
    pub kind: String,
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editable_source_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_generation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width_px: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_px: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LiveTranscriptAttachment {
    pub id: String,
    pub rows: Vec<LiveTranscriptRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct LiveTranscriptRow {
    pub role: ChatItemRole,
    pub text: String,
}

/// One canonical timeline item. Identities come from durable entry ids / turn
/// tickets — never from rendered text, timestamps alone, or array position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ChatItem {
    // PartialEq used by headless regression tests.
    pub id: String,
    pub role: ChatItemRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_index: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<ChatLane>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_runs: Option<Vec<ToolRunState>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<ChatSegment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_artifacts: Option<Vec<UiArtifact>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_whisper: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_rounds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_attachments: Option<Vec<ChatMediaAttachment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_context: Option<HostTurnContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reactions: Option<Vec<MessageReaction>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_transcripts: Option<Vec<LiveTranscriptAttachment>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<TranscriptCoordinate>,
}

impl ChatItem {
    pub fn is_streaming(&self) -> bool {
        self.streaming.unwrap_or(false)
    }
}

/// Explicit causal links the renderer must not reinvent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatRelationship {
    ToolBelongsTo {
        tool_run_id: String,
        turn_id: String,
        item_id: String,
    },
    AgentSpawn {
        work_id: String,
        parent_turn_id: Option<String>,
        parent_item_id: Option<String>,
    },
    AgentFollowUp {
        work_id: String,
        reply_to_work_id: String,
        parent_turn_id: Option<String>,
        parent_item_id: Option<String>,
    },
    ProposalAnchored {
        proposal_id: String,
        anchor_item_id: Option<String>,
        origin_proposal_id: String,
    },
}

/// Thin host-supplied proposal facts for turn placement (no daemon fetch in core).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub struct ProposalSourceFact {
    pub authority_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_entry_seq: Option<u64>,
    pub through_entry_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub struct ProposalFact {
    pub proposal_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_runtime_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_authority_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding_session_id: Option<String>,
    #[serde(default)]
    pub sources: Vec<ProposalSourceFact>,
}

/// Worker coordinates used for placement (insertion order preserved by the Vec).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "snake_case")]
pub struct WorkerPlacementFact {
    pub work_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_item_id: Option<String>,
}

/// Full worker-lane link projected on the snapshot (spawn + synthesis lifecycle).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct WorkerLinkFact {
    pub work_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to_work_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthesis_item_id: Option<String>,
    #[serde(default)]
    pub synthesis_delivered: bool,
}

/// One proposal or worker in mixed spawn order for a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentPlacementEntry {
    Proposal {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        #[serde(rename = "originProposalId")]
        origin_proposal_id: String,
        #[serde(rename = "nodeId", default, skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
    },
    Worker {
        #[serde(rename = "workId")]
        work_id: String,
        #[serde(rename = "nodeId", default, skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
    },
}

/// Agents/tasks anchored to an initiating turn (or `None` for earlier/unknown).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct AgentPlacementGroup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_item_id: Option<String>,
    pub entries: Vec<AgentPlacementEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct ChatSnapshot {
    pub session_id: String,
    pub authority_id: String,
    pub revision: ChatRevision,
    pub items: Vec<ChatItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<ChatRelationship>,
    /// Turn placement for proposals/workers. Thin ids — UI joins full rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_groups: Vec<AgentPlacementGroup>,
    /// Worker-lane spawn + synthesis links (thin facts for the host map).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub workers: Vec<WorkerLinkFact>,
    /// Reconnect/dedup cursors keyed by turn id. Never presentation identity.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub last_seq_by_turn: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(rename_all = "camelCase")]
pub struct BeginTurnRequest {
    pub turn_id: String,
    pub user_item_id: String,
    pub assistant_item_id: String,
    pub user_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<ChatLane>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_profile_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_attachments: Option<Vec<ChatMediaAttachment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatCommand {
    OpenSession {
        session_id: String,
        authority_id: String,
    },
    /// Replace/merge durable history into the session projection.
    ApplyHistory {
        session_id: String,
        authority_id: String,
        turns: Vec<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lane: Option<ChatLane>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ask_job_id: Option<String>,
        /// When set, reject if the session revision does not match (stale async).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
        /// When true, replace local durable rows instead of merging (cold load).
        #[serde(default)]
        replace: bool,
        /// Optional local base from the renderer (optimistic/streaming rows) when
        /// the host session has not yet been seeded via BeginTurn.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        local_items: Option<Vec<ChatItem>>,
    },
    /// Pure projection helper — does not mutate session state.
    ProjectHistory {
        /// Plain strings so clients can project before a durable authority id is known.
        session_id: String,
        authority_id: String,
        /// Accept daemon history JSON without requiring prefixed id validation at the
        /// application boundary (daemon responses already validate; UI fixtures may not).
        turns: Vec<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lane: Option<ChatLane>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ask_job_id: Option<String>,
    },
    BeginTurn {
        session_id: String,
        request: BeginTurnRequest,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
    },
    ApplyStreamEvent {
        session_id: String,
        envelope: TurnStreamEnvelopeV3,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
        /// When the turn has no shell yet, create one under this item id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        promote_item_id: Option<String>,
    },
    SettleTurn {
        session_id: String,
        turn_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
        /// External agent streams keep reconnect cursors across prompts.
        #[serde(default)]
        reusable_stream: bool,
    },
    LinkWorker {
        session_id: String,
        work_id: String,
        parent_turn_id: Option<String>,
        parent_item_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to_work_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
    },
    /// Create or patch the worker-lane synthesis follow-up bubble.
    EnsureWorkerFollowUp {
        session_id: String,
        work_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        /// Stable id when omitted: `worker-synth:{work_id}`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status_line: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        streaming: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
    },
    /// Clear live chrome on the handoff ack bubble after synthesis arrives.
    FinalizeWorkerHandoff {
        session_id: String,
        item_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
    },
    /// Unlock stream settle after synthesis is painted.
    MarkWorkerSynthesisDelivered {
        session_id: String,
        work_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
    },
    /// Card/history recovery: ensure follow-up, write content, finalize, mark delivered.
    DeliverWorkerSynthesis {
        session_id: String,
        work_id: String,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tools: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
    },
    /// Store host-supplied proposal facts and refresh placement on the snapshot.
    ApplyProposals {
        session_id: String,
        proposals: Vec<ProposalFact>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visible_item_ids: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
    },
    /// Update painted/visible item ids used by turn-anchor placement.
    ApplyVisibleItems {
        session_id: String,
        visible_item_ids: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_revision: Option<ChatRevision>,
    },
    /// Pure placement projection. Uses `items` when provided, otherwise session items.
    ProjectAgentPlacement {
        session_id: String,
        authority_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        items: Option<Vec<ChatItem>>,
        #[serde(default)]
        proposals: Vec<ProposalFact>,
        #[serde(default)]
        workers: Vec<WorkerPlacementFact>,
        #[serde(default)]
        visible_item_ids: Vec<String>,
    },
    SwitchFocus {
        session_id: String,
    },
    DisposeSession {
        session_id: String,
    },
    Snapshot {
        session_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatUpdate {
    Snapshot {
        snapshot: ChatSnapshot,
    },
    Projected {
        items: Vec<ChatItem>,
    },
    AgentPlacement {
        groups: Vec<AgentPlacementGroup>,
    },
    Rejected {
        session_id: String,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revision: Option<ChatRevision>,
    },
    Disposed {
        session_id: String,
    },
}

/// Types exported into the application schema catalog.
pub fn schema_export_names() -> &'static [&'static str] {
    &[
        "ChatRevision",
        "ChatItemRole",
        "ChatLane",
        "TranscriptCoordinate",
        "ChatSegment",
        "ChatItem",
        "ChatRelationship",
        "ProposalFact",
        "WorkerPlacementFact",
        "WorkerLinkFact",
        "AgentPlacementEntry",
        "AgentPlacementGroup",
        "ChatSnapshot",
        "BeginTurnRequest",
        "ChatCommand",
        "ChatUpdate",
    ]
}
