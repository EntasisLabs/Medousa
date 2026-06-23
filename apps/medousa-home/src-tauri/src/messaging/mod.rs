pub mod product_config;
pub mod secrets;

use product_config::{
    load_product_config_summary, save_channel_product_config, ChannelConfigSave,
    ProductConfigSummary,
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
pub fn messaging_secret_status(secret_id: String) -> Result<bool, String> {
    secrets::secret_is_set(secret_id.trim())
}

#[tauri::command]
pub fn messaging_save_secret(secret_id: String, value: Option<String>) -> Result<(), String> {
    secrets::save_secret(secret_id.trim(), value)?;
    crate::channel_adapters::sync_channel_adapters(None)?;
    Ok(())
}

#[tauri::command]
pub fn messaging_clear_secret(secret_id: String) -> Result<(), String> {
    secrets::clear_secret(secret_id.trim())?;
    crate::channel_adapters::sync_channel_adapters(None)?;
    Ok(())
}

#[tauri::command]
pub fn messaging_read_secret(secret_id: String) -> Result<Option<String>, String> {
    secrets::load_secret_value(secret_id.trim())
}
