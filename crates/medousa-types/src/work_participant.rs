//! Narrow authenticated provider participation. These commands never issue grants.
use crate::{SessionId, coordination::*, work_coordination::*, work_unit::*};
use serde::{Deserialize, Serialize};

/// Source selection is checked against a visible, owned, governed chat.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PeerProposalIntent {
    /// Stable request key; reuse for exact retries.
    pub request_key: String,
    /// Omit to use the workshop's saved preference and available fallbacks.
    #[serde(default)]
    pub runtime: Option<ExternalPeerRuntime>,
    /// An explicit owned project; omission preserves legacy chat binding.
    #[serde(default)]
    pub forge_work_id: Option<String>,
    pub instructions: String,
    /// Exclusive lower bound from native discovery.
    pub after_entry_seq: u64,
    /// Inclusive committed upper bound from native discovery.
    pub through_entry_seq: u64,
    /// Result-only owner-chat continuation; must be false for HTTP work participants.
    pub continue_owner: bool,
    /// Exact adoptable native session; omit to create fresh work.
    pub existing_agent_session_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", deny_unknown_fields)]
pub enum WorkParticipantQuery {
    #[serde(rename = "work.graph")]
    Graph { query: WorkGraphQuery },
    #[serde(rename = "work.get")]
    Get { work_unit_id: String },
    #[serde(rename = "work.events")]
    Events { query: WorkEventsQuery },
    #[serde(rename = "work.coordination")]
    Coordination { query: WorkCoordinationQuery },
    #[serde(rename = "peer.discover")]
    Discover { session_id: SessionId },
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(tag = "action", deny_unknown_fields)]
pub enum WorkParticipantMutation {
    #[serde(rename = "work.record")]
    Record { command: WorkGraphCommand },
    #[serde(rename = "work.coordinate")]
    Coordinate { input: WorkCoordinationInput },
    #[serde(rename = "peer.propose")]
    Propose {
        session_id: SessionId,
        intent: PeerProposalIntent,
    },
}

/// Bounded native result; no transport acknowledgment is promoted to completion.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkParticipantResponse {
    pub result: serde_json::Value,
}
