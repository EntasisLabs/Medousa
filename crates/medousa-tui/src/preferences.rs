pub use medousa_types::llm_defaults::{
    resolve_llm_base_url, resolve_llm_model, resolve_llm_provider,
};
pub const DEFAULT_HOST_BUS_MAX_TOOL_ROUNDS: usize = 8;
pub const DEFAULT_GENERAL_MAX_TOOL_ROUNDS: usize = 30;
pub const DEFAULT_CODER_MAX_TOOL_ROUNDS: usize = 100;
pub const DEFAULT_ACTIVATION_TOOL_INTENT_MAX_ROUNDS: usize = 12;
pub const DEFAULT_ACTIVATION_SHORT_TURN_MAX_TOOL_ROUNDS: usize = 1;
pub const DEFAULT_CONTINUATION_MAX_TOOL_ROUNDS: usize = 4;
pub const DEFAULT_MAX_TEXT_ONLY_STUCK_CONTINUES: usize = 10;
pub const DEFAULT_CLASSIFIER_RESTRICTED_MAX_TOOL_ROUNDS: usize = 1;

pub fn default_host_turn_bus_mode_label() -> &'static str {
    "auto"
}

pub const DEFAULT_RETRY_RUNTIME_MAX_ROUNDS: usize = 10;
pub use crate::tui::settings::{
    OPERATOR_RETRY_LIMIT_MAX as RETRY_LIMIT_MAX, OPERATOR_RETRY_LIMIT_MIN as RETRY_LIMIT_MIN,
    OPERATOR_ROUND_LIMIT_MAX as ROUND_LIMIT_MAX, OPERATOR_ROUND_LIMIT_MIN as ROUND_LIMIT_MIN,
};
pub const REASONING_EFFORT_DEFAULT: &str = "default";

/// Normalize user-facing reasoning effort to an optional provider keyword.
/// Empty, `default`, and `auto` mean "do not set reasoning_effort on the request".
pub fn normalize_reasoning_effort(value: &str) -> Option<String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "default" | "auto" => None,
        other => Some(other.to_string()),
    }
}

pub fn normalize_reasoning_effort_value(value: &str) -> String {
    normalize_reasoning_effort(value).unwrap_or_else(|| REASONING_EFFORT_DEFAULT.to_string())
}
