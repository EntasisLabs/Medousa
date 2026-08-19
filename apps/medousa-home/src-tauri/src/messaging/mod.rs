pub mod daemon_slots;
pub mod product_config;
pub mod secrets;

use product_config::{
    ChannelConfigSave, ProductConfigSummary, load_product_config_summary,
    save_channel_product_config,
};

#[tauri::command]
pub fn messaging_load_product_config_summary() -> Result<ProductConfigSummary, String> {
    load_product_config_summary()
}

#[tauri::command]
pub fn messaging_save_channel_config(request: ChannelConfigSave) -> Result<(), String> {
    save_channel_product_config(request)
}

#[tauri::command]
pub fn messaging_secret_status(_secret_id: String) -> Result<bool, String> {
    Err("provider and channel secrets are stored by the engine — use Settings integrations".into())
}

#[tauri::command]
pub fn messaging_save_secret(_secret_id: String, _value: Option<String>) -> Result<(), String> {
    Err("provider and channel secrets are stored by the engine — use Settings integrations".into())
}

#[tauri::command]
pub fn messaging_clear_secret(_secret_id: String) -> Result<(), String> {
    Err("provider and channel secrets are stored by the engine — use Settings integrations".into())
}

#[tauri::command]
pub fn messaging_read_secret(_secret_id: String) -> Result<Option<String>, String> {
    Err("secret values are never returned to Home".into())
}
