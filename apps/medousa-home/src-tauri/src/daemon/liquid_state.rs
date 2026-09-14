use medousa_types::{LiquidComponentStateResponse, PutLiquidComponentStateRequest};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::{DaemonState, workshop_http};

fn path(session_id: &str, message_id: &str, node_id: &str, instance_id: &str) -> String {
    format!(
        "/v1/sessions/{}/liquid-state/{}/{}/{}",
        urlencoding::encode(session_id.trim()),
        urlencoding::encode(message_id.trim()),
        urlencoding::encode(node_id.trim()),
        urlencoding::encode(instance_id.trim())
    )
}

#[tauri::command]
pub async fn liquid_state_get(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: String,
    message_id: String,
    node_id: String,
    instance_id: String,
) -> Result<LiquidComponentStateResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if _embedded_state.client_if_active().await?.is_some() {
        return medousa::liquid_state::get(&session_id, &message_id, &node_id, &instance_id)
            .map_err(|error| error.to_string());
    }
    workshop_http::get_json(&state, &path(&session_id, &message_id, &node_id, &instance_id)).await
}

#[tauri::command]
pub async fn liquid_state_put(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    session_id: String,
    message_id: String,
    node_id: String,
    instance_id: String,
    request: PutLiquidComponentStateRequest,
) -> Result<LiquidComponentStateResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if _embedded_state.client_if_active().await?.is_some() {
        return medousa::liquid_state::put(
            &session_id,
            &message_id,
            &node_id,
            &instance_id,
            request,
        )
        .map_err(|error| error.to_string());
    }
    workshop_http::put_json(
        &state,
        &path(&session_id, &message_id, &node_id, &instance_id),
        &request,
    )
    .await
}
