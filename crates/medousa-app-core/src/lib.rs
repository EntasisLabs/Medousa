//! Renderer-agnostic Medousa application core.
//!
//! This crate owns chat timeline lifecycle, live/persisted reconciliation,
//! stable identities, and tool/agent causal relationships. Renderers consume
//! [`contract::ChatSnapshot`] updates; they do not reimplement reconciliation.
//!
//! Daemon wire types and workshop authority stay in `medousa-types` / the
//! daemon. This crate projects those facts into application state.

pub mod agents;
pub mod contract;
pub mod display;
pub mod history;
pub mod merge;
pub mod parts;
pub mod session;
pub mod stream;
pub mod wire_history;

pub use contract::{
    AgentPlacementEntry, AgentPlacementGroup, BeginTurnRequest, ChatCommand, ChatItem, ChatItemRole,
    ChatLane, ChatRelationship, ChatRevision, ChatSnapshot, ChatUpdate, ProposalFact,
    TranscriptCoordinate, WorkerPlacementFact,
};
pub use agents::project_agent_placement;
pub use history::{project_history, ProjectHistoryOptions};
pub use merge::{dedupe_messages_by_id, merge_transcript};
pub use session::{guard_focused_session, ChatCore, ChatCoreError};
pub use stream::{apply_v3_envelope_to_item, fold_v3_envelopes, v3_event_promotes_chat_message};
