//! One native-admitted, result-only model wake; model output is never a grant.
use crate::{SessionRef, TranscriptEntryRef, work_provider::WorkProviderRequestInput};
use serde::{Deserialize, Serialize};

pub const WORK_COORDINATOR_SESSION_PREFIX: &str = "ses_work_coordinator_";
pub const WORK_COORDINATOR_TURN_PREFIX: &str = "work_coordinator_";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkCoordinatorWake {
    pub conversation_id: String,
    pub request_id: String,
    pub input: WorkProviderRequestInput,
    pub scope_digest: String,
    pub session: SessionRef,
    pub provider: String,
    pub model: String,
    pub response_depth_mode: String,
    pub reasoning_effort: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct WorkCoordinatorAttempt {
    pub turn_id: String,
    pub event_id: String,
    pub prompt_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorkCoordinatorDecision {
    pub entry: TranscriptEntryRef,
    pub content_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorkCoordinatorWakeRecord {
    pub wake: WorkCoordinatorWake,
    pub attempt: Option<WorkCoordinatorAttempt>,
    pub decision: Option<WorkCoordinatorDecision>,
    pub blocked_reason: Option<String>,
    pub revision: u64,
}
