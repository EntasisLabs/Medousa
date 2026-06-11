use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::medousa_paths::{load_tui_defaults_summary, tui_defaults_path};

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
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
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
                file.screen = Some(WizardScreen::Screen2);
            }
            WizardScreen::Screen2 => {
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
            WizardScreen::Screen3 => file.screen = Some(WizardScreen::Screen2),
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
