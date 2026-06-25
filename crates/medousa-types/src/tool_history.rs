use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::workflow::WorkflowRunRequest;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolHistoryRunEntry {
    pub entry_id: String,
    pub session_id: String,
    pub slice_id: String,
    pub turn_index: usize,
    pub tool_round: usize,
    pub run_id: String,
    pub tool_name: String,
    pub status: String,
    pub input_summary: String,
    pub sanitized_input: Value,
    pub args_hash: String,
    pub redacted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_preview: Option<String>,
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolHistoryListQuery {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub session_limit: Option<usize>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tool_filter: Option<String>,
    #[serde(default)]
    pub keyword: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolHistoryListResponse {
    pub count: usize,
    pub runs: Vec<ToolHistoryRunEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolHistorySliceRef {
    pub session_id: String,
    pub slice_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_round: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowFromSliceRequest {
    pub refs: Vec<ToolHistorySliceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowFromSliceResponse {
    pub workflow_id: Option<String>,
    pub draft: WorkflowRunRequest,
    pub promoted_count: usize,
    pub notes: Vec<String>,
}
