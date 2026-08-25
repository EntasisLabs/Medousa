pub mod product_config;
pub mod secrets;

use product_config::{
    load_product_config_summary, save_channel_product_config, ChannelConfigSave,
    ProductConfigSummary,
};
use tauri::State;

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
    require_native_secret_authority()?;
    secrets::secret_is_set(secret_id.trim())
}

#[tauri::command]
pub async fn messaging_save_secret(
    embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    secret_id: String,
    value: Option<String>,
) -> Result<(), String> {
    require_native_secret_authority()?;
    let secret_id = secret_id.trim();
    let changes_inference_route = secret_id.starts_with("base_url_");
    let previous = secrets::load_secret_value(secret_id)?;
    secrets::save_secret(secret_id, value)?;
    #[cfg(target_os = "ios")]
    if changes_inference_route {
        if let Err(error) = embedded_state
            .reconfigure_active(&crate::medousa_paths::load_tui_defaults())
            .await
        {
            let _ = secrets::save_secret(secret_id, previous);
            let _ = embedded_state
                .reconfigure_active(&crate::medousa_paths::load_tui_defaults())
                .await;
            return Err(error);
        }
    }
    #[cfg(not(target_os = "ios"))]
    let _ = (embedded_state, previous, changes_inference_route);
    crate::channel_adapters::sync_channel_adapters(None)?;
    Ok(())
}

#[tauri::command]
pub async fn messaging_clear_secret(
    embedded_state: State<'_, crate::embedded_daemon::EmbeddedDaemonState>,
    secret_id: String,
) -> Result<(), String> {
    messaging_save_secret(embedded_state, secret_id, None).await
}

#[tauri::command]
pub fn messaging_read_secret(secret_id: String) -> Result<Option<String>, String> {
    require_native_secret_authority()?;
    secrets::load_secret_value(secret_id.trim())
}

fn require_native_secret_authority() -> Result<(), String> {
    #[cfg(target_os = "ios")]
    if !matches!(
        crate::active_workshop::resolve()?,
        crate::active_workshop::ActiveWorkshopTarget::EmbeddedPersonal
    ) {
        return Err(
            "native secrets belong to Embedded Personal; use the selected workshop daemon"
                .to_string(),
        );
    }
    Ok(())
}
