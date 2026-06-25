use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowStepSpec {
    Grapheme {
        id: String,
        source: String,
    },
    Prompt {
        id: String,
        user_prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        system_prompt: Option<String>,
    },
    Mcp {
        id: String,
        server_id: String,
        tool_name: String,
        #[serde(default)]
        args: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        effect_class: Option<String>,
    },
    ToolReplay {
        id: String,
        tool_name: String,
        #[serde(default)]
        input: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slice_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_round: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_id: Option<String>,
        #[serde(default)]
        requires_confirm: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRunRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    pub steps: Vec<WorkflowStepSpec>,
    #[serde(default = "default_on_failure")]
    pub on_failure: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue: Option<String>,
}

fn default_strategy() -> String {
    "sequential".to_string()
}

fn default_mode() -> String {
    "default".to_string()
}

fn default_on_failure() -> String {
    "stop".to_string()
}

impl WorkflowStepSpec {
    pub fn id(&self) -> &str {
        match self {
            Self::Grapheme { id, .. } | Self::Prompt { id, .. } | Self::Mcp { id, .. }
            | Self::ToolReplay { id, .. } => id,
        }
    }
}
