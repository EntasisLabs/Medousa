//! Payload-free projections. Work facts must come from a domain authority, not
//! tool success or a context-selection heuristic. No prompt/evidence prose here.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorkContextProjection {
    pub work_id: String,
    /// Exact authoritative lifecycle label, never a claim that the user's goal passed.
    pub phase: String,
    pub progress: Option<String>,
    pub blockers: Vec<String>,
    pub decisions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct UserContextProjection {
    pub work: Option<WorkContextProjection>,
    pub pressure: String,
    pub deferred_items: usize,
    pub accounting: String,
}

impl UserContextProjection {
    /// Accounting/pressure fluctuations are detail, not principal-facing updates.
    pub fn meaningful_change_from(&self, previous: &Self) -> bool {
        self.work != previous.work || self.deferred_items != previous.deferred_items
    }
}
