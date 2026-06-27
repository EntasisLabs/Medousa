use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::workflow::WorkflowRunRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorkflowPlanRequest {
    pub goal: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorkflowScheduleSuggestion {
    pub cron_expr: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct WorkflowPlanResponse {
    pub goal: String,
    pub confidence: String,
    pub execute_with: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_workflow: Option<WorkflowRunRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_schedule: Option<WorkflowScheduleSuggestion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_tool_input: Option<Value>,
    pub notes: Vec<String>,
    pub assumptions: Vec<String>,
}
