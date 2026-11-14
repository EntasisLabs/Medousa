use serde::Serialize;
use std::path::PathBuf;

use medousa_types::stage_routing::StageRoutingMatrix;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MedousaConfigPaths {
    pub data_dir: String,
    pub data_dir_source: String,
    pub config_dir: String,
    pub vault_dir: String,
    pub product_config: String,
    pub tui_defaults: String,
    pub capabilities: String,
    pub mcp_gateway: String,
}

pub use medousa_desktop_host::defaults::{
    read_tui_defaults_file, tui_defaults_path, FavoriteModelDto, TuiDefaultsDto, TuiDefaultsFile,
    TuiDefaultsSummary, VoicePresetDto,
};

fn medousa_data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn medousa_config_dir() -> PathBuf {
    crate::paths::medousa_config_dir()
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
        activation_direct_answer_max_prompt_chars: file.activation_direct_answer_max_prompt_chars,
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
        reasoning_effort: file.reasoning_effort.clone(),
        stage_routing: file.stage_routing.clone(),
        web_search_preferred_provider: file.web_search_preferred_provider.clone(),
        web_search_try_fallbacks: file.web_search_try_fallbacks,
        stt_provider: file.stt_provider.clone(),
        stt_model: file.stt_model.clone(),
        stt_base_url: file.stt_base_url.clone(),
        work_card_hide_after_hours: file.work_card_hide_after_hours,
        work_card_wipe_after_days: file.work_card_wipe_after_days,
        favorite_models: file.favorite_models.clone(),
        active_voice_id: file.active_voice_id.clone(),
        custom_voice_presets: file.custom_voice_presets.clone(),
        inference_profiles: file.inference_profiles.clone(),
        shell_agent_tools_enabled: file.shell_agent_tools_enabled,
        vault_git_enabled: file.vault_git_enabled,
        shell_network_default: file.shell_network_default,
        shell_timeout_ms: file.shell_timeout_ms,
        shell_max_output_bytes: file.shell_max_output_bytes,
        shell_allowed_binaries: file.shell_allowed_binaries.clone(),
        shell_writable_roots: file.shell_writable_roots.clone(),
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
    file.activation_direct_answer_max_prompt_chars = dto.activation_direct_answer_max_prompt_chars;
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
    file.reasoning_effort = dto.reasoning_effort.clone();
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
            .filter(|entry| !entry.provider.trim().is_empty() && !entry.model.trim().is_empty())
            .take(8)
            .collect();
        if filtered.is_empty() {
            None
        } else {
            Some(filtered)
        }
    });
    file.active_voice_id = dto
        .active_voice_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    file.custom_voice_presets = normalize_voice_presets(dto.custom_voice_presets.clone());
    file.inference_profiles = dto.inference_profiles.clone();
    file.shell_agent_tools_enabled = dto.shell_agent_tools_enabled;
    file.vault_git_enabled = dto.vault_git_enabled;
    file.shell_network_default = dto.shell_network_default;
    file.shell_timeout_ms = dto.shell_timeout_ms;
    file.shell_max_output_bytes = dto.shell_max_output_bytes;
    file.shell_allowed_binaries = dto.shell_allowed_binaries.clone().map(|bins| {
        bins.into_iter()
            .map(|bin| bin.trim().to_string())
            .filter(|bin| !bin.is_empty())
            .collect()
    });
    file.shell_writable_roots = dto.shell_writable_roots.clone().map(|roots| {
        roots
            .into_iter()
            .map(|root| root.trim().to_string())
            .filter(|root| !root.is_empty())
            .collect()
    });
    if dto.stage_routing.is_some() {
        file.stage_routing = dto.stage_routing.clone();
    }
    file.stage_routing = align_saved_stage_routing(
        file.stage_routing.clone(),
        file.provider.as_deref(),
        file.model.as_deref(),
    );
}

#[tauri::command]
pub fn connection_runbook_path(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;

    // Packaged `.app` / installers: file is listed under bundle.resources.
    if let Ok(resource_dir) = app.path().resource_dir() {
        let packaged = [
            resource_dir.join("resources/docs/runbooks/connection-reliability.md"),
            resource_dir.join("docs/runbooks/connection-reliability.md"),
            resource_dir.join("connection-reliability.md"),
        ];
        for path in packaged {
            if path.is_file() {
                return Ok(path.display().to_string());
            }
        }
    }

    // `tauri dev` / contributor checkout: resolve against the repo + local resources copy.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest.join("resources/docs/runbooks/connection-reliability.md"),
        manifest.join("../../docs/runbooks/connection-reliability.md"),
        manifest.join("../../../docs/runbooks/connection-reliability.md"),
    ];
    for path in candidates {
        if path.is_file() {
            return Ok(path.display().to_string());
        }
    }
    Err(
        "Connection troubleshooting guide not found on this install. See docs/runbooks/connection-reliability.md in the Medousa repo."
            .to_string(),
    )
}

#[tauri::command]
pub fn medousa_config_paths() -> MedousaConfigPaths {
    let data = medousa_data_dir();
    let config = medousa_config_dir();
    MedousaConfigPaths {
        data_dir: data.display().to_string(),
        data_dir_source: crate::paths::medousa_data_dir_source().to_string(),
        config_dir: config.display().to_string(),
        vault_dir: data.join("vault").display().to_string(),
        product_config: data.join("product_config.json").display().to_string(),
        tui_defaults: data.join("tui_defaults.json").display().to_string(),
        capabilities: config.join("capabilities.toml").display().to_string(),
        mcp_gateway: config.join("mcp-gateway.toml").display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::should_unload_local_worker;

    #[test]
    fn switching_away_from_local_unloads_worker() {
        assert!(should_unload_local_worker(
            Some("medousa-local"),
            Some("gemma-local"),
            "openai",
            "gpt-5",
        ));
    }

    #[test]
    fn changing_local_model_unloads_worker() {
        assert!(should_unload_local_worker(
            Some("medousa-local"),
            Some("gemma-local"),
            "medousa-local",
            "qwen-local",
        ));
    }

    #[test]
    fn retaining_local_model_keeps_worker_warm() {
        assert!(!should_unload_local_worker(
            Some("medousa-local"),
            Some("gemma-local"),
            "medousa-local",
            "gemma-local",
        ));
    }

    #[test]
    fn remote_provider_changes_do_not_touch_local_worker() {
        assert!(!should_unload_local_worker(
            Some("anthropic"),
            Some("claude"),
            "openai",
            "gpt-5",
        ));
    }

    #[test]
    fn summary_aligns_uniform_deepseek_matrix_to_luna() {
        let matrix = serde_json::to_value(super::StageRoutingMatrix::default_for(
            "deepseek",
            "deepseek-v4-flash",
        ))
        .expect("matrix");
        let aligned =
            super::align_saved_stage_routing(Some(matrix), Some("openai"), Some("gpt-5.6-luna"))
                .expect("aligned");
        assert_eq!(aligned["final_response"]["provider"], "openai");
        assert_eq!(aligned["final_response"]["model"], "gpt-5.6-luna");
        assert_eq!(aligned["extractor"]["provider"], "openai");
    }
}

fn align_saved_stage_routing(
    raw: Option<serde_json::Value>,
    provider: Option<&str>,
    model: Option<&str>,
) -> Option<serde_json::Value> {
    let Some(provider) = provider.map(str::trim).filter(|value| !value.is_empty()) else {
        return raw;
    };
    let Some(model) = model.map(str::trim).filter(|value| !value.is_empty()) else {
        return raw;
    };
    match raw {
        None => serde_json::to_value(StageRoutingMatrix::default_for(provider, model)).ok(),
        Some(value) => match serde_json::from_value::<StageRoutingMatrix>(value.clone()) {
            Ok(matrix) => serde_json::to_value(matrix.aligned_with_host(provider, model)).ok(),
            Err(_) => Some(value),
        },
    }
}

#[tauri::command]
pub fn load_tui_defaults_summary() -> TuiDefaultsSummary {
    let file = read_tui_defaults_file();
    TuiDefaultsSummary {
        provider: file.provider.clone(),
        model: file.model.clone(),
        response_depth_mode: file.response_depth_mode,
        reasoning_effort: file.reasoning_effort,
        stage_routing: align_saved_stage_routing(
            file.stage_routing,
            file.provider.as_deref(),
            file.model.as_deref(),
        ),
        favorite_models: file.favorite_models,
        active_voice_id: file.active_voice_id,
        custom_voice_presets: file.custom_voice_presets,
    }
}

#[tauri::command]
pub fn load_tui_defaults() -> TuiDefaultsDto {
    file_to_dto(&read_tui_defaults_file())
}

pub fn tui_defaults_dto_from_value(value: &serde_json::Value) -> TuiDefaultsDto {
    let file: TuiDefaultsFile = serde_json::from_value(value.clone()).unwrap_or_default();
    file_to_dto(&file)
}

pub fn tui_defaults_value_from_dto(dto: &TuiDefaultsDto) -> serde_json::Value {
    let mut file = TuiDefaultsFile::default();
    apply_dto_to_file(&mut file, dto);
    serde_json::to_value(&file).unwrap_or_else(|_| serde_json::json!({}))
}

pub fn global_host_tui_defaults_path() -> PathBuf {
    crate::paths::default_medousa_data_dir().join("tui_defaults.json")
}

pub fn global_host_tui_defaults_migrated_marker() -> PathBuf {
    crate::paths::default_medousa_data_dir().join("tui_defaults.json.host-migrated")
}

/// One-time import of legacy host-global `tui_defaults.json` into the active engine.
pub fn migrate_global_tui_defaults_if_needed() -> Result<bool, String> {
    let legacy = global_host_tui_defaults_path();
    let marker = global_host_tui_defaults_migrated_marker();
    if !legacy.is_file() || marker.is_file() {
        return Ok(false);
    }
    let raw = std::fs::read_to_string(&legacy).map_err(|err| err.to_string())?;
    if raw.trim().is_empty() {
        return Ok(false);
    }
    let _: TuiDefaultsDto = {
        let file: TuiDefaultsFile =
            serde_json::from_str(&raw).map_err(|err| format!("legacy defaults invalid: {err}"))?;
        file_to_dto(&file)
    };
    std::fs::rename(&legacy, &marker).map_err(|err| err.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn persist_tui_defaults(dto: TuiDefaultsDto) -> Result<(), String> {
    let mut file = read_tui_defaults_file();
    apply_dto_to_file(&mut file, &dto);
    write_tui_defaults_file(&file)
}

#[tauri::command]
pub async fn persist_tui_runtime_prefs(
    provider: String,
    model: String,
    response_depth_mode: String,
    reasoning_effort: Option<String>,
    stage_routing: Option<serde_json::Value>,
) -> Result<(), String> {
    let mut file = read_tui_defaults_file();
    let unload_local_worker = should_unload_local_worker(
        file.provider.as_deref(),
        file.model.as_deref(),
        &provider,
        &model,
    );
    file.provider = Some(provider);
    file.model = Some(model);
    file.response_depth_mode = Some(response_depth_mode);
    if let Some(effort) = reasoning_effort {
        file.reasoning_effort = Some(effort);
    }
    if let Some(matrix) = stage_routing {
        file.stage_routing = Some(matrix);
    }
    file.stage_routing = align_saved_stage_routing(
        file.stage_routing.clone(),
        file.provider.as_deref(),
        file.model.as_deref(),
    );
    write_tui_defaults_file(&file)?;
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    if unload_local_worker {
        crate::workshop_runtime::stop_local_brain_bounded(
            crate::workshop_registry::PERSONAL_WORKSHOP_ID,
        )
        .await?;
    }
    #[cfg(any(target_os = "ios", target_os = "android"))]
    let _ = unload_local_worker;
    Ok(())
}

fn should_unload_local_worker(
    previous_provider: Option<&str>,
    previous_model: Option<&str>,
    next_provider: &str,
    next_model: &str,
) -> bool {
    previous_provider.is_some_and(|provider| provider.trim() == "medousa-local")
        && (next_provider.trim() != "medousa-local"
            || previous_model.map(str::trim) != Some(next_model.trim()))
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

pub(crate) fn normalize_voice_presets(
    presets: Option<Vec<VoicePresetDto>>,
) -> Option<Vec<VoicePresetDto>> {
    let presets = presets?;
    let filtered: Vec<VoicePresetDto> = presets
        .into_iter()
        .filter_map(|entry| {
            let id = entry.id.trim();
            let name = entry.name.trim();
            let voice_appendix = entry.voice_appendix.trim();
            if id.is_empty() || name.is_empty() || voice_appendix.is_empty() {
                return None;
            }
            if id == "default" || id == "direct" {
                return None;
            }
            Some(VoicePresetDto {
                id: id.to_string(),
                name: name.to_string(),
                description: entry
                    .description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                voice_appendix: voice_appendix.to_string(),
            })
        })
        .take(8)
        .collect();
    if filtered.is_empty() {
        None
    } else {
        Some(filtered)
    }
}

#[tauri::command]
pub fn persist_tui_voice_prefs(
    active_voice_id: String,
    custom_voice_presets: Option<Vec<VoicePresetDto>>,
) -> Result<(), String> {
    let mut file = read_tui_defaults_file();
    let trimmed = active_voice_id.trim();
    file.active_voice_id = if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    };
    file.custom_voice_presets = normalize_voice_presets(custom_voice_presets);
    write_tui_defaults_file(&file)
}
