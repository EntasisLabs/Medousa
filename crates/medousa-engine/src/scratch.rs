//! Ephemeral turn scratch types referenced by the stream sink port.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnScratchPhase {
    #[default]
    Discover,
    Execute,
    Finalize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkerDelegateScratch {
    pub work_id: String,
    pub intent: String,
}

/// Ephemeral working memory for one host or worker tool-loop execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnScratchpad {
    pub goal: String,
    pub phase: TurnScratchPhase,
    pub step: usize,
    pub last_tools: Vec<String>,
    pub last_error: Option<String>,
    pub open_gaps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegate: Option<WorkerDelegateScratch>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub round_digests: Vec<String>,
}
