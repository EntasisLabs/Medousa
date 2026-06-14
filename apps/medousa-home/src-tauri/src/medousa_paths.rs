use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MedousaConfigPaths {
    pub data_dir: String,
    pub config_dir: String,
    pub product_config: String,
    pub tui_defaults: String,
    pub capabilities: String,
    pub mcp_gateway: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteModelDto {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TuiDefaultsSummary {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub response_depth_mode: Option<String>,
    pub stage_routing: Option<serde_json::Value>,
    pub favorite_models: Option<Vec<FavoriteModelDto>>,
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
    pub stage_routing: Option<serde_json::Value>,
    pub web_search_preferred_provider: Option<String>,
    pub web_search_try_fallbacks: Option<bool>,
    pub stt_provider: Option<String>,
    pub stt_model: Option<String>,
    pub stt_base_url: Option<String>,
    pub work_card_hide_after_hours: Option<u32>,
    pub work_card_wipe_after_days: Option<u32>,
    pub favorite_models: Option<Vec<FavoriteModelDto>>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
struct TuiDefaultsFile {
    #[serde(default)]
    backend: Option<String>,
    #[serde(default)]
    theme_id: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    env_overrides: Option<String>,
    #[serde(default)]
    allowed_modules: Option<Vec<String>>,
    #[serde(default)]
    tool_call_mode: Option<String>,
    #[serde(default)]
    max_tool_rounds: Option<usize>,
    #[serde(default)]
    host_bus_max_tool_rounds: Option<usize>,
    #[serde(default)]
    host_turn_bus_mode: Option<String>,
    #[serde(default)]
    activation_tool_intent_max_rounds: Option<usize>,
    #[serde(default)]
    activation_short_turn_max_tool_rounds: Option<usize>,
    #[serde(default)]
    continuation_max_tool_rounds: Option<usize>,
    #[serde(default)]
    max_text_only_stuck_continues: Option<usize>,
    #[serde(default)]
    classifier_restricted_max_tool_rounds: Option<usize>,
    #[serde(default)]
    thinking_capture: Option<bool>,
    #[serde(default)]
    stasis_otel_enabled: Option<bool>,
    #[serde(default)]
    thinking_max_lines: Option<usize>,
    #[serde(default)]
    activation_direct_answer_max_prompt_chars: Option<usize>,
    #[serde(default)]
    activation_long_session_turn_threshold: Option<usize>,
    #[serde(default)]
    activation_long_session_max_prompt_chars: Option<usize>,
    #[serde(default)]
    slice_hot_window_turns: Option<usize>,
    #[serde(default)]
    slice_cold_window_turns: Option<usize>,
    #[serde(default)]
    retry_runtime_max_retries: Option<usize>,
    #[serde(default)]
    retry_runtime_max_rounds: Option<usize>,
    #[serde(default)]
    verifier_min_citation_coverage: Option<f32>,
    #[serde(default)]
    verifier_min_avg_support_strength: Option<f32>,
    #[serde(default)]
    verifier_min_supported_claim_ratio: Option<f32>,
    #[serde(default)]
    verifier_min_claim_support_strength: Option<f32>,
    #[serde(default)]
    response_depth_mode: Option<String>,
    #[serde(default)]
    stage_routing: Option<serde_json::Value>,
    #[serde(default)]
    web_search_preferred_provider: Option<String>,
    #[serde(default)]
    web_search_try_fallbacks: Option<bool>,
    #[serde(default)]
    stt_provider: Option<String>,
    #[serde(default)]
    stt_model: Option<String>,
    #[serde(default)]
    stt_base_url: Option<String>,
    #[serde(default)]
    work_card_hide_after_hours: Option<u32>,
    #[serde(default)]
    work_card_wipe_after_days: Option<u32>,
    #[serde(default)]
    favorite_models: Option<Vec<FavoriteModelDto>>,
    #[serde(default)]
    command_usage_counts: Option<serde_json::Value>,
    #[serde(default)]
    surreal_endpoint: Option<String>,
    #[serde(default)]
    surreal_username: Option<String>,
    #[serde(default)]
    surreal_password: Option<String>,
    #[serde(default)]
    surreal_namespace: Option<String>,
    #[serde(default)]
    surreal_database: Option<String>,
}

fn medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

fn medousa_config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

pub(crate) fn tui_defaults_path() -> PathBuf {
    medousa_data_dir().join("tui_defaults.json")
}

pub(crate) fn read_tui_defaults_file() -> TuiDefaultsFile {
    let path = tui_defaults_path();
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_tui_defaults_file(defaults: &TuiDefaultsFile) -> Result<(), String> {
    let path = tui_defaults_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let json = serde_json::to_string_pretty(defaults).map_err(|err| err.to_string())?;
    std::fs::write(path, json).map_err(|err| err.to_string())
}

fn file_to_dto(file: &TuiDefaultsFile) -> TuiDefaultsDto {
    TuiDefaultsDto {
        backend: file.backend.clone(),
        theme_id: file.theme_id.clone(),
        provider: file.provider.clone(),
        model: file.model.clone(),
        base_url: file.base_url.clone(),
        env_overrides: file.env_overrides.clone(),
        allowed_modules: file.allowed_modules.clone(),
        tool_call_mode: file.tool_call_mode.clone(),
        max_tool_rounds: file.max_tool_rounds,
        host_bus_max_tool_rounds: file.host_bus_max_tool_rounds,
        host_turn_bus_mode: file.host_turn_bus_mode.clone(),
        activation_tool_intent_max_rounds: file.activation_tool_intent_max_rounds,
        activation_short_turn_max_tool_rounds: file.activation_short_turn_max_tool_rounds,
        continuation_max_tool_rounds: file.continuation_max_tool_rounds,
        max_text_only_stuck_continues: file.max_text_only_stuck_continues,
        classifier_restricted_max_tool_rounds: file.classifier_restricted_max_tool_rounds,
        thinking_capture: file.thinking_capture,
        stasis_otel_enabled: file.stasis_otel_enabled,
        thinking_max_lines: file.thinking_max_lines,
        activation_direct_answer_max_prompt_chars: file
            .activation_direct_answer_max_prompt_chars,
        activation_long_session_turn_threshold: file.activation_long_session_turn_threshold,
        activation_long_session_max_prompt_chars: file.activation_long_session_max_prompt_chars,
        slice_hot_window_turns: file.slice_hot_window_turns,
        slice_cold_window_turns: file.slice_cold_window_turns,
        retry_runtime_max_retries: file.retry_runtime_max_retries,
        retry_runtime_max_rounds: file.retry_runtime_max_rounds,
        verifier_min_citation_coverage: file.verifier_min_citation_coverage,
        verifier_min_avg_support_strength: file.verifier_min_avg_support_strength,
        verifier_min_supported_claim_ratio: file.verifier_min_supported_claim_ratio,
        verifier_min_claim_support_strength: file.verifier_min_claim_support_strength,
        response_depth_mode: file.response_depth_mode.clone(),
        stage_routing: file.stage_routing.clone(),
        web_search_preferred_provider: file.web_search_preferred_provider.clone(),
        web_search_try_fallbacks: file.web_search_try_fallbacks,
        stt_provider: file.stt_provider.clone(),
        stt_model: file.stt_model.clone(),
        stt_base_url: file.stt_base_url.clone(),
        work_card_hide_after_hours: file.work_card_hide_after_hours,
        work_card_wipe_after_days: file.work_card_wipe_after_days,
        favorite_models: file.favorite_models.clone(),
    }
}

fn apply_dto_to_file(file: &mut TuiDefaultsFile, dto: &TuiDefaultsDto) {
    file.backend = dto.backend.clone();
    file.theme_id = dto.theme_id.clone();
    file.provider = dto.provider.clone();
    file.model = dto.model.clone();
    file.base_url = dto.base_url.clone();
    file.env_overrides = dto.env_overrides.clone();
    file.allowed_modules = dto.allowed_modules.clone();
    file.tool_call_mode = dto.tool_call_mode.clone();
    file.max_tool_rounds = dto.max_tool_rounds;
    file.host_bus_max_tool_rounds = dto.host_bus_max_tool_rounds;
    file.host_turn_bus_mode = dto.host_turn_bus_mode.clone();
    file.activation_tool_intent_max_rounds = dto.activation_tool_intent_max_rounds;
    file.activation_short_turn_max_tool_rounds = dto.activation_short_turn_max_tool_rounds;
    file.continuation_max_tool_rounds = dto.continuation_max_tool_rounds;
    file.max_text_only_stuck_continues = dto.max_text_only_stuck_continues;
    file.classifier_restricted_max_tool_rounds = dto.classifier_restricted_max_tool_rounds;
    file.thinking_capture = dto.thinking_capture;
    file.stasis_otel_enabled = dto.stasis_otel_enabled;
    file.thinking_max_lines = dto.thinking_max_lines;
    file.activation_direct_answer_max_prompt_chars =
        dto.activation_direct_answer_max_prompt_chars;
    file.activation_long_session_turn_threshold = dto.activation_long_session_turn_threshold;
    file.activation_long_session_max_prompt_chars = dto.activation_long_session_max_prompt_chars;
    file.slice_hot_window_turns = dto.slice_hot_window_turns;
    file.slice_cold_window_turns = dto.slice_cold_window_turns;
    file.retry_runtime_max_retries = dto.retry_runtime_max_retries;
    file.retry_runtime_max_rounds = dto.retry_runtime_max_rounds;
    file.verifier_min_citation_coverage = dto.verifier_min_citation_coverage;
    file.verifier_min_avg_support_strength = dto.verifier_min_avg_support_strength;
    file.verifier_min_supported_claim_ratio = dto.verifier_min_supported_claim_ratio;
    file.verifier_min_claim_support_strength = dto.verifier_min_claim_support_strength;
    file.response_depth_mode = dto.response_depth_mode.clone();
    file.web_search_preferred_provider = dto
        .web_search_preferred_provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    file.web_search_try_fallbacks = dto.web_search_try_fallbacks;
    file.stt_provider = dto
        .stt_provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    file.stt_model = dto
        .stt_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    file.stt_base_url = dto
        .stt_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    file.work_card_hide_after_hours = dto.work_card_hide_after_hours;
    file.work_card_wipe_after_days = dto.work_card_wipe_after_days;
    file.favorite_models = dto.favorite_models.clone().and_then(|entries| {
        let filtered: Vec<FavoriteModelDto> = entries
            .into_iter()
            .filter(|entry| {
                !entry.provider.trim().is_empty() && !entry.model.trim().is_empty()
            })
            .take(8)
            .collect();
        if filtered.is_empty() {
            None
        } else {
            Some(filtered)
        }
    });
    if dto.stage_routing.is_some() {
        file.stage_routing = dto.stage_routing.clone();
    }
}

#[tauri::command]
pub fn medousa_config_paths() -> MedousaConfigPaths {
    let data = medousa_data_dir();
    let config = medousa_config_dir();
    MedousaConfigPaths {
        data_dir: data.display().to_string(),
        config_dir: config.display().to_string(),
        product_config: data.join("product_config.json").display().to_string(),
        tui_defaults: data.join("tui_defaults.json").display().to_string(),
        capabilities: config.join("capabilities.toml").display().to_string(),
        mcp_gateway: config.join("mcp-gateway.toml").display().to_string(),
    }
}

#[tauri::command]
pub fn load_tui_defaults_summary() -> TuiDefaultsSummary {
    let file = read_tui_defaults_file();
    TuiDefaultsSummary {
        provider: file.provider,
        model: file.model,
        response_depth_mode: file.response_depth_mode,
        stage_routing: file.stage_routing,
        favorite_models: file.favorite_models,
    }
}

#[tauri::command]
pub fn load_tui_defaults() -> TuiDefaultsDto {
    file_to_dto(&read_tui_defaults_file())
}

#[tauri::command]
pub fn persist_tui_defaults(dto: TuiDefaultsDto) -> Result<(), String> {
    let mut file = read_tui_defaults_file();
    apply_dto_to_file(&mut file, &dto);
    write_tui_defaults_file(&file)
}

#[tauri::command]
pub fn persist_tui_runtime_prefs(
    provider: String,
    model: String,
    response_depth_mode: String,
    stage_routing: Option<serde_json::Value>,
) -> Result<(), String> {
    let mut file = read_tui_defaults_file();
    file.provider = Some(provider);
    file.model = Some(model);
    file.response_depth_mode = Some(response_depth_mode);
    if let Some(matrix) = stage_routing {
        file.stage_routing = Some(matrix);
    }
    write_tui_defaults_file(&file)
}

#[tauri::command]
pub fn persist_tui_favorite_models(models: Vec<FavoriteModelDto>) -> Result<(), String> {
    let mut file = read_tui_defaults_file();
    file.favorite_models = Some(
        models
            .into_iter()
            .filter(|entry| !entry.provider.trim().is_empty() && !entry.model.trim().is_empty())
            .take(8)
            .collect(),
    );
    write_tui_defaults_file(&file)
}
