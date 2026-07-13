use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::inference::InferenceProfilesConfig;
use crate::stage_routing::StageRoutingMatrix;
use crate::turn::TurnPart;
use crate::turn::TurnSliceSummary;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct ConversationTurn {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<TurnPart>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice_summary: Option<TurnSliceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TuiDefaults {
    pub backend: Option<String>,
    pub theme_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub env_overrides: Option<String>,
    pub allowed_modules: Option<Vec<String>>,
    pub tool_call_mode: Option<String>,
    pub max_tool_rounds: Option<usize>,
    pub host_bus_max_tool_rounds: Option<usize>,
    pub host_turn_bus_mode: Option<String>,
    pub activation_tool_intent_max_rounds: Option<usize>,
    pub activation_short_turn_max_tool_rounds: Option<usize>,
    pub continuation_max_tool_rounds: Option<usize>,
    pub max_text_only_stuck_continues: Option<usize>,
    pub classifier_restricted_max_tool_rounds: Option<usize>,
    pub thinking_capture: Option<bool>,
    #[serde(default)]
    pub stasis_otel_enabled: Option<bool>,
    pub thinking_max_lines: Option<usize>,
    pub activation_direct_answer_max_prompt_chars: Option<usize>,
    pub activation_long_session_turn_threshold: Option<usize>,
    pub activation_long_session_max_prompt_chars: Option<usize>,
    pub slice_hot_window_turns: Option<usize>,
    pub slice_cold_window_turns: Option<usize>,
    pub retry_runtime_max_retries: Option<usize>,
    pub retry_runtime_max_rounds: Option<usize>,
    pub verifier_min_citation_coverage: Option<f32>,
    pub verifier_min_avg_support_strength: Option<f32>,
    pub verifier_min_supported_claim_ratio: Option<f32>,
    pub verifier_min_claim_support_strength: Option<f32>,
    pub response_depth_mode: Option<String>,
    pub reasoning_effort: Option<String>,
    pub stage_routing: Option<StageRoutingMatrix>,
    pub command_usage_counts: Option<std::collections::HashMap<String, u64>>,
    pub web_search_preferred_provider: Option<String>,
    pub web_search_try_fallbacks: Option<bool>,
    #[serde(default)]
    pub work_card_hide_after_hours: Option<u32>,
    #[serde(default)]
    pub work_card_wipe_after_days: Option<u32>,
    pub surreal_endpoint: Option<String>,
    pub surreal_username: Option<String>,
    pub surreal_password: Option<String>,
    pub surreal_namespace: Option<String>,
    pub surreal_database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_profiles: Option<InferenceProfilesConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt_base_url: Option<String>,
    /// Master switch for `cognition_shell_*` agent tools (default off — sensitive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_agent_tools_enabled: Option<bool>,
    /// Charter ceiling for shell network access (default off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_network_default: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_max_output_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_allowed_binaries: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_writable_roots: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct SessionHistorySummary {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub turns: usize,
    pub verification_runs: usize,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub last_verification_timestamp: Option<DateTime<Utc>>,
    pub last_verification_confidence: Option<f32>,
    pub last_verification_coverage: Option<f32>,
    pub last_verification_verified: Option<bool>,
    pub preview: String,
}

impl SessionHistorySummary {
    pub fn without_verification_fields(mut self) -> Self {
        self.verification_runs = 0;
        self.last_verification_timestamp = None;
        self.last_verification_confidence = None;
        self.last_verification_coverage = None;
        self.last_verification_verified = None;
        self
    }
}

impl ConversationTurn {
    pub fn plain(
        role: impl Into<String>,
        content: String,
        timestamp: DateTime<Utc>,
        tool_names: Vec<String>,
        answer_state: Option<String>,
    ) -> Self {
        Self {
            role: role.into(),
            content,
            timestamp,
            tool_names,
            answer_state,
            parts: None,
            slice_summary: None,
        }
    }
}
