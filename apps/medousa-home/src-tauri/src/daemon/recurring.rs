use crate::daemon::types::{
    DeleteRecurringResponse, RecurringListResponse, RegisterRecurringPromptRequest,
    RegisterRecurringResponse, UpdateRecurringRequest, UpdateRecurringResponse,
};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn recurring_list(
    state: State<'_, DaemonState>,
    enabled_only: Option<bool>,
) -> Result<RecurringListResponse, String> {
    let path = if let Some(value) = enabled_only {
        format!("/v1/recurring?enabled_only={value}")
    } else {
        "/v1/recurring".to_string()
    };
    workshop_http::get_json(&state, &path).await
}

#[tauri::command]
pub async fn recurring_register_prompt(
    state: State<'_, DaemonState>,
    request: RegisterRecurringPromptRequest,
) -> Result<RegisterRecurringResponse, String> {
    workshop_http::post_json(&state, "/v1/recurring/prompt", &request).await
}

#[tauri::command]
pub async fn recurring_update(
    state: State<'_, DaemonState>,
    recurring_id: String,
    request: UpdateRecurringRequest,
) -> Result<UpdateRecurringResponse, String> {
    workshop_http::patch_json(
        &state,
        &format!("/v1/recurring/{}", recurring_id.trim()),
        &request,
    )
    .await
}

#[tauri::command]
pub async fn recurring_delete(
    state: State<'_, DaemonState>,
    recurring_id: String,
) -> Result<DeleteRecurringResponse, String> {
    workshop_http::delete_json(&state, &format!("/v1/recurring/{}", recurring_id.trim())).await
}
