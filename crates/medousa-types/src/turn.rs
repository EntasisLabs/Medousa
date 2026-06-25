use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TurnArtifactRef {
    pub role: String,
    pub content_type: String,
    pub byte_size: usize,
    pub hash64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnPart {
    Text {
        markdown: String,
    },
    /// Ephemeral-style progress captured between tool rounds (not the final answer).
    Progress {
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
        started_at: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        finished_at: Option<DateTime<Utc>>,
    },
    Handoff {
        handoff_kind: String,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        work_id: Option<String>,
    },
    UserMedia {
        media_id: String,
        mime: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        byte_size: Option<u64>,
    },
    AttachmentRef {
        artifact_id: String,
        mime: String,
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        byte_size: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TurnSliceSummary {
    pub goal: String,
    pub tool_rounds: usize,
    pub tools: Vec<String>,
    pub outcomes: Vec<String>,
    pub failures: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scratch_phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegate_work_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegate_intent: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_gaps: Vec<String>,
}
