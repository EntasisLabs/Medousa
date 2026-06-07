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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TuiDefaultsSummary {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub response_depth_mode: Option<String>,
    pub stage_routing: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
struct TuiDefaultsFile {
    provider: Option<String>,
    model: Option<String>,
    response_depth_mode: Option<String>,
    stage_routing: Option<serde_json::Value>,
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

fn tui_defaults_path() -> PathBuf {
    medousa_data_dir().join("tui_defaults.json")
}

fn read_tui_defaults_file() -> TuiDefaultsFile {
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
    }
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
