//! Shared mode status / enable bridge.

use serde::{Deserialize, Serialize};

use super::window_request::WorkshopRequest;
use super::workshop_http;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedModeStatusResponse {
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_at: Option<String>,
    pub root_profile_id: String,
    pub general_profile_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetSharedModeRequest {
    pub mode: String,
}

#[tauri::command]
pub async fn shared_mode_status(
    state: WorkshopRequest,
) -> Result<SharedModeStatusResponse, String> {
    workshop_http::get_json(&state, "/v1/shared-mode").await
}

#[tauri::command]
pub async fn shared_mode_set(
    state: WorkshopRequest,
    mode: String,
) -> Result<SharedModeStatusResponse, String> {
    let trimmed = mode.trim().to_ascii_lowercase();
    if trimmed != "shared" && trimmed != "personal" {
        return Err("mode must be 'shared' or 'personal'".to_string());
    }
    workshop_http::put_json(
        &state,
        "/v1/shared-mode",
        &SetSharedModeRequest { mode: trimmed },
    )
    .await
}
