use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteModelDto {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoicePresetDto {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub voice_appendix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InferenceTargetDto {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InferenceProfileDto {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default)]
    pub fallbacks: Vec<InferenceTargetDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InferenceProfilesDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<InferenceProfileDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision: Option<InferenceProfileDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_generation: Option<InferenceProfileDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stt: Option<InferenceProfileDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TuiDefaultsSummary {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub response_depth_mode: Option<String>,
    pub reasoning_effort: Option<String>,
    pub stage_routing: Option<serde_json::Value>,
    pub favorite_models: Option<Vec<FavoriteModelDto>>,
    pub active_voice_id: Option<String>,
    pub custom_voice_presets: Option<Vec<VoicePresetDto>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TuiDefaultsDto {
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
    pub stage_routing: Option<serde_json::Value>,
    pub web_search_preferred_provider: Option<String>,
    pub web_search_try_fallbacks: Option<bool>,
    pub stt_provider: Option<String>,
    pub stt_model: Option<String>,
    pub stt_base_url: Option<String>,
    pub work_card_hide_after_hours: Option<u32>,
    pub work_card_wipe_after_days: Option<u32>,
    pub favorite_models: Option<Vec<FavoriteModelDto>>,
    pub active_voice_id: Option<String>,
    pub custom_voice_presets: Option<Vec<VoicePresetDto>>,
    pub inference_profiles: Option<InferenceProfilesDto>,
    pub shell_agent_tools_enabled: Option<bool>,
    pub vault_git_enabled: Option<bool>,
    pub shell_network_default: Option<bool>,
    pub shell_timeout_ms: Option<u64>,
    pub shell_max_output_bytes: Option<u64>,
    pub shell_allowed_binaries: Option<Vec<String>>,
    pub shell_writable_roots: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct TuiDefaultsFile {
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub theme_id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub env_overrides: Option<String>,
    #[serde(default)]
    pub allowed_modules: Option<Vec<String>>,
    #[serde(default)]
    pub tool_call_mode: Option<String>,
    #[serde(default)]
    pub max_tool_rounds: Option<usize>,
    #[serde(default)]
    pub host_bus_max_tool_rounds: Option<usize>,
    #[serde(default)]
    pub host_turn_bus_mode: Option<String>,
    #[serde(default)]
    pub activation_tool_intent_max_rounds: Option<usize>,
    #[serde(default)]
    pub activation_short_turn_max_tool_rounds: Option<usize>,
    #[serde(default)]
    pub continuation_max_tool_rounds: Option<usize>,
    #[serde(default)]
    pub max_text_only_stuck_continues: Option<usize>,
    #[serde(default)]
    pub classifier_restricted_max_tool_rounds: Option<usize>,
    #[serde(default)]
    pub thinking_capture: Option<bool>,
    #[serde(default)]
    pub stasis_otel_enabled: Option<bool>,
    #[serde(default)]
    pub thinking_max_lines: Option<usize>,
    #[serde(default)]
    pub activation_direct_answer_max_prompt_chars: Option<usize>,
    #[serde(default)]
    pub activation_long_session_turn_threshold: Option<usize>,
    #[serde(default)]
    pub activation_long_session_max_prompt_chars: Option<usize>,
    #[serde(default)]
    pub slice_hot_window_turns: Option<usize>,
    #[serde(default)]
    pub slice_cold_window_turns: Option<usize>,
    #[serde(default)]
    pub retry_runtime_max_retries: Option<usize>,
    #[serde(default)]
    pub retry_runtime_max_rounds: Option<usize>,
    #[serde(default)]
    pub verifier_min_citation_coverage: Option<f32>,
    #[serde(default)]
    pub verifier_min_avg_support_strength: Option<f32>,
    #[serde(default)]
    pub verifier_min_supported_claim_ratio: Option<f32>,
    #[serde(default)]
    pub verifier_min_claim_support_strength: Option<f32>,
    #[serde(default)]
    pub response_depth_mode: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub stage_routing: Option<serde_json::Value>,
    #[serde(default)]
    pub web_search_preferred_provider: Option<String>,
    #[serde(default)]
    pub web_search_try_fallbacks: Option<bool>,
    #[serde(default)]
    pub stt_provider: Option<String>,
    #[serde(default)]
    pub stt_model: Option<String>,
    #[serde(default)]
    pub stt_base_url: Option<String>,
    #[serde(default)]
    pub work_card_hide_after_hours: Option<u32>,
    #[serde(default)]
    pub work_card_wipe_after_days: Option<u32>,
    #[serde(default)]
    pub favorite_models: Option<Vec<FavoriteModelDto>>,
    #[serde(default)]
    pub active_voice_id: Option<String>,
    #[serde(default)]
    pub custom_voice_presets: Option<Vec<VoicePresetDto>>,
    #[serde(default)]
    pub inference_profiles: Option<InferenceProfilesDto>,
    #[serde(default)]
    pub shell_agent_tools_enabled: Option<bool>,
    #[serde(default)]
    pub vault_git_enabled: Option<bool>,
    #[serde(default)]
    pub shell_network_default: Option<bool>,
    #[serde(default)]
    pub shell_timeout_ms: Option<u64>,
    #[serde(default)]
    pub shell_max_output_bytes: Option<u64>,
    #[serde(default)]
    pub shell_allowed_binaries: Option<Vec<String>>,
    #[serde(default)]
    pub shell_writable_roots: Option<Vec<String>>,
    #[serde(default)]
    pub command_usage_counts: Option<serde_json::Value>,
    #[serde(default)]
    pub surreal_endpoint: Option<String>,
    #[serde(default)]
    pub surreal_username: Option<String>,
    #[serde(default)]
    pub surreal_password: Option<String>,
    #[serde(default)]
    pub surreal_namespace: Option<String>,
    #[serde(default)]
    pub surreal_database: Option<String>,
}

pub fn tui_defaults_path() -> PathBuf {
    crate::paths::medousa_data_dir().join("tui_defaults.json")
}

pub fn read_tui_defaults_file() -> TuiDefaultsFile {
    let path = tui_defaults_path();
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}
