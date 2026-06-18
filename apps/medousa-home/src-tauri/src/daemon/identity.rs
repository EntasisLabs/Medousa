use crate::daemon::types::IdentityContextRequest;
use serde_json::Value;
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn identity_get_context(
    state: State<'_, DaemonState>,
    request: IdentityContextRequest,
) -> Result<Value, String> {
    workshop_http::post_json(&state, "/v1/identity/context", &request).await
}
