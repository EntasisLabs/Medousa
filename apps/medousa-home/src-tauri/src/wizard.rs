use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::daemon_service::{daemon_start, daemon_wait_healthy, DaemonWaitHealthRequest};
use crate::medousa_paths::{load_tui_defaults_summary, persist_tui_defaults, tui_defaults_path, TuiDefaultsDto};
use crate::messaging::messaging_save_secret;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WizardLifecycleState {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "completed")]
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WizardScreen {
    Migration,
    Screen1,
    Screen2,
    Screen3,
    Completion,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WizardFile {
    #[serde(default)]
    pub state: Option<WizardLifecycleState>,
    #[serde(default)]
    pub screen: Option<WizardScreen>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub screen1_model: Option<String>,
    #[serde(default)]
    pub screen2_skipped: Option<bool>,
    #[serde(default)]
    pub screen3_skipped: Option<bool>,
    #[serde(default)]
    pub migration_from: Option<String>,
    #[serde(default)]
    pub rerun: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardBootstrap {
    pub visible: bool,
    pub mode: String,
    pub screen: WizardScreen,
    pub existing_provider: Option<String>,
    pub existing_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardApplyScreen1Request {
    pub path: String,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_start_core")]
    pub start_core: bool,
}

fn default_start_core() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardApplyScreen1Result {
    pub core_ready: bool,
    pub core_message: String,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WizardAdvanceRequest {
    pub action: String,
    #[serde(default)]
    pub screen1_model: Option<String>,
    #[serde(default)]
    pub screen2_skipped: Option<bool>,
    #[serde(default)]
    pub screen3_skipped: Option<bool>,
}

fn medousa_data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn wizard_path() -> PathBuf {
    medousa_data_dir().join("wizard.json")
}

fn product_config_path() -> PathBuf {
    medousa_data_dir().join("product_config.json")
}

fn read_wizard_file() -> Option<WizardFile> {
    let path = wizard_path();
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_wizard_file(file: &WizardFile) -> Result<(), String> {
    let path = wizard_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let json = serde_json::to_string_pretty(file).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())
}

fn legacy_install_detected() -> bool {
    if tui_defaults_path().is_file() {
        let summary = load_tui_defaults_summary();
        if summary
            .provider
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
        {
            return true;
        }
    }
    product_config_path().is_file()
}

fn existing_runtime_summary() -> (Option<String>, Option<String>) {
    let summary = load_tui_defaults_summary();
    (
        summary.provider.filter(|value| !value.trim().is_empty()),
        summary.model.filter(|value| !value.trim().is_empty()),
    )
}

fn bootstrap_from_file(file: &WizardFile) -> WizardBootstrap {
    let (existing_provider, existing_model) = existing_runtime_summary();
    if file.state == Some(WizardLifecycleState::Completed) && file.rerun != Some(true) {
        return WizardBootstrap {
            visible: false,
            mode: "none".to_string(),
            screen: WizardScreen::Screen1,
            existing_provider,
            existing_model,
        };
    }

    let screen = file.screen.clone().unwrap_or(WizardScreen::Screen1);
    let mode = if file.rerun == Some(true) {
        "rerun".to_string()
    } else if screen == WizardScreen::Migration {
        "migration".to_string()
    } else {
        "fresh".to_string()
    };

    WizardBootstrap {
        visible: true,
        mode,
        screen,
        existing_provider,
        existing_model,
    }
}

fn ensure_fresh_wizard() -> Result<WizardFile, String> {
    let file = WizardFile {
        state: Some(WizardLifecycleState::Active),
        screen: Some(WizardScreen::Screen1),
        completed_at: None,
        screen1_model: None,
        screen2_skipped: None,
        screen3_skipped: None,
        migration_from: None,
        rerun: Some(false),
    };
    write_wizard_file(&file)?;
    Ok(file)
}

fn ensure_migration_wizard() -> Result<WizardFile, String> {
    let file = WizardFile {
        state: Some(WizardLifecycleState::Active),
        screen: Some(WizardScreen::Migration),
        completed_at: None,
        screen1_model: None,
        screen2_skipped: None,
        screen3_skipped: None,
        migration_from: Some("legacy-tui".to_string()),
        rerun: Some(false),
    };
    write_wizard_file(&file)?;
    Ok(file)
}

#[tauri::command]
pub fn wizard_bootstrap() -> Result<WizardBootstrap, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        return Ok(WizardBootstrap {
            visible: false,
            mode: "none".to_string(),
            screen: WizardScreen::Screen1,
            existing_provider: None,
            existing_model: None,
        });
    }

    if let Some(file) = read_wizard_file() {
        return Ok(bootstrap_from_file(&file));
    }

    if legacy_install_detected() {
        let file = ensure_migration_wizard()?;
        return Ok(bootstrap_from_file(&file));
    }

    let file = ensure_fresh_wizard()?;
    Ok(bootstrap_from_file(&file))
}

#[tauri::command]
pub fn wizard_begin_rerun() -> Result<WizardBootstrap, String> {
    let (existing_provider, existing_model) = existing_runtime_summary();
    let file = WizardFile {
        state: Some(WizardLifecycleState::Active),
        screen: Some(WizardScreen::Screen1),
        completed_at: None,
        screen1_model: existing_provider.clone(),
        screen2_skipped: None,
        screen3_skipped: None,
        migration_from: None,
        rerun: Some(true),
    };
    write_wizard_file(&file)?;
    Ok(WizardBootstrap {
        visible: true,
        mode: "rerun".to_string(),
        screen: WizardScreen::Screen1,
        existing_provider,
        existing_model,
    })
}

#[tauri::command]
pub fn wizard_advance(request: WizardAdvanceRequest) -> Result<WizardBootstrap, String> {
    let mut file = read_wizard_file().ok_or_else(|| "wizard state missing".to_string())?;
    if file.state != Some(WizardLifecycleState::Active) {
        return Err("wizard is not active".to_string());
    }

    let screen = file.screen.clone().unwrap_or(WizardScreen::Screen1);
    match request.action.as_str() {
        "continue" => match screen {
            WizardScreen::Migration => {
                file.state = Some(WizardLifecycleState::Completed);
                file.screen = Some(WizardScreen::Completion);
                file.completed_at = Some(Utc::now());
            }
            WizardScreen::Screen1 => {
                if let Some(model) = request
                    .screen1_model
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    file.screen1_model = Some(model.to_string());
                }
                file.screen2_skipped = Some(true);
                let mobile_client = request
                    .screen1_model
                    .as_deref()
                    .map(str::trim)
                    == Some("mobile-client");
                if mobile_client {
                    file.screen3_skipped = Some(true);
                    file.screen = Some(WizardScreen::Completion);
                } else {
                    file.screen = Some(WizardScreen::Screen3);
                }
            }
            WizardScreen::Screen2 => {
                file.screen2_skipped = Some(true);
                file.screen = Some(WizardScreen::Screen3);
            }
            WizardScreen::Screen3 => {
                file.screen = Some(WizardScreen::Completion);
            }
            WizardScreen::Completion => {
                file.state = Some(WizardLifecycleState::Completed);
                file.completed_at = Some(Utc::now());
            }
        },
        "skip" => match screen {
            WizardScreen::Screen1 => {
                file.screen2_skipped = Some(true);
                file.screen = Some(WizardScreen::Screen3);
            }
            WizardScreen::Screen2 => {
                file.screen2_skipped = Some(true);
                file.screen = Some(WizardScreen::Screen3);
            }
            WizardScreen::Screen3 => {
                file.screen3_skipped = Some(true);
                file.screen = Some(WizardScreen::Completion);
            }
            _ => return Err(format!("screen {:?} cannot be skipped", screen)),
        },
        "back" => match screen {
            WizardScreen::Screen2 => file.screen = Some(WizardScreen::Screen1),
            WizardScreen::Screen3 => file.screen = Some(WizardScreen::Screen1),
            _ => {}
        },
        other => return Err(format!("unknown wizard action: {other}")),
    }

    if let Some(skipped) = request.screen2_skipped {
        file.screen2_skipped = Some(skipped);
    }
    if let Some(skipped) = request.screen3_skipped {
        file.screen3_skipped = Some(skipped);
    }

    write_wizard_file(&file)?;
    Ok(bootstrap_from_file(&file))
}

#[tauri::command]
pub async fn wizard_apply_screen1(
    request: WizardApplyScreen1Request,
) -> Result<WizardApplyScreen1Result, String> {
    let provider = request.provider.trim().to_ascii_lowercase();
    let model = request.model.trim().to_string();
    if provider.is_empty() || model.is_empty() {
        return Err("provider and model are required".to_string());
    }

    let base_url = request
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut dto = TuiDefaultsDto {
        backend: Some("surreal-mem".to_string()),
        provider: Some(provider.clone()),
        model: Some(model.clone()),
        base_url: base_url.clone(),
        response_depth_mode: Some("standard".to_string()),
        ..Default::default()
    };

    if let Ok(raw) = std::fs::read_to_string(tui_defaults_path()) {
        if let Ok(existing) = serde_json::from_str::<TuiDefaultsDto>(&raw) {
            dto = TuiDefaultsDto {
                backend: existing.backend.or(dto.backend),
                theme_id: existing.theme_id,
                allowed_modules: existing.allowed_modules,
                tool_call_mode: existing.tool_call_mode,
                max_tool_rounds: existing.max_tool_rounds,
                host_bus_max_tool_rounds: existing.host_bus_max_tool_rounds,
                host_turn_bus_mode: existing.host_turn_bus_mode,
                activation_tool_intent_max_rounds: existing.activation_tool_intent_max_rounds,
                activation_short_turn_max_tool_rounds: existing.activation_short_turn_max_tool_rounds,
                continuation_max_tool_rounds: existing.continuation_max_tool_rounds,
                max_text_only_stuck_continues: existing.max_text_only_stuck_continues,
                classifier_restricted_max_tool_rounds: existing.classifier_restricted_max_tool_rounds,
                thinking_capture: existing.thinking_capture,
                stasis_otel_enabled: existing.stasis_otel_enabled,
                thinking_max_lines: existing.thinking_max_lines,
                activation_direct_answer_max_prompt_chars: existing
                    .activation_direct_answer_max_prompt_chars,
                activation_long_session_turn_threshold: existing
                    .activation_long_session_turn_threshold,
                activation_long_session_max_prompt_chars: existing
                    .activation_long_session_max_prompt_chars,
                slice_hot_window_turns: existing.slice_hot_window_turns,
                slice_cold_window_turns: existing.slice_cold_window_turns,
                retry_runtime_max_retries: existing.retry_runtime_max_retries,
                retry_runtime_max_rounds: existing.retry_runtime_max_rounds,
                verifier_min_citation_coverage: existing.verifier_min_citation_coverage,
                verifier_min_avg_support_strength: existing.verifier_min_avg_support_strength,
                verifier_min_supported_claim_ratio: existing.verifier_min_supported_claim_ratio,
                verifier_min_claim_support_strength: existing.verifier_min_claim_support_strength,
                web_search_preferred_provider: existing.web_search_preferred_provider,
                web_search_try_fallbacks: existing.web_search_try_fallbacks,
                stt_provider: existing.stt_provider,
                stt_model: existing.stt_model,
                stt_base_url: existing.stt_base_url,
                stage_routing: existing.stage_routing,
                env_overrides: existing.env_overrides,
                provider: Some(provider.clone()),
                model: Some(model.clone()),
                base_url: base_url.or(existing.base_url),
                response_depth_mode: existing.response_depth_mode.or(dto.response_depth_mode),
                reasoning_effort: existing.reasoning_effort.or(dto.reasoning_effort),
                work_card_hide_after_hours: existing.work_card_hide_after_hours,
                work_card_wipe_after_days: existing.work_card_wipe_after_days,
                favorite_models: existing.favorite_models,
                active_voice_id: existing.active_voice_id,
                custom_voice_presets: existing.custom_voice_presets,
                inference_profiles: existing.inference_profiles,
                shell_agent_tools_enabled: existing.shell_agent_tools_enabled,
                shell_network_default: existing.shell_network_default,
                shell_timeout_ms: existing.shell_timeout_ms,
                shell_max_output_bytes: existing.shell_max_output_bytes,
                shell_allowed_binaries: existing.shell_allowed_binaries,
                shell_writable_roots: existing.shell_writable_roots,
            };
        }
    }

    persist_tui_defaults(dto)?;

    if provider != "ollama" {
        if let Some(key) = request
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            messaging_save_secret("api_key".to_string(), Some(key.to_string()))?;
        }
    }

    let mut core_ready = true;
    let mut core_message = "Provider saved".to_string();

    if request.start_core {
        let private_brain = request.path.eq_ignore_ascii_case("offline");
        let start = daemon_start(Some(crate::daemon_service::DaemonStartRequest {
            private_brain,
            public_bind: None,
        }))
        .await?;
        core_message = start.message;
        let wait = daemon_wait_healthy(Some(DaemonWaitHealthRequest {
            timeout_seconds: 120,
            poll_ms: 2000,
        }))
        .await?;
        core_ready = wait.ok;
        core_message = wait.message;
    }

    let _path = request.path.trim();

    Ok(WizardApplyScreen1Result {
        core_ready,
        core_message,
        provider,
        model,
    })
}

#[tauri::command]
pub fn wizard_complete() -> Result<WizardBootstrap, String> {
    let mut file = read_wizard_file().unwrap_or(WizardFile {
        state: Some(WizardLifecycleState::Active),
        screen: Some(WizardScreen::Completion),
        ..Default::default()
    });
    file.state = Some(WizardLifecycleState::Completed);
    file.screen = Some(WizardScreen::Completion);
    file.completed_at = Some(Utc::now());
    file.rerun = Some(false);
    write_wizard_file(&file)?;
    Ok(bootstrap_from_file(&file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wizard_screen_serializes_camel_case() {
        let screen = WizardScreen::Screen1;
        let json = serde_json::to_string(&screen).expect("serialize");
        assert_eq!(json, "\"screen1\"");
    }
}
