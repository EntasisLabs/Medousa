use crate::daemon::types::{
    DeleteRecurringResponse, RecurringDeliveryResponse, RecurringListResponse,
    RecurringRunsResponse, RegisterRecurringPromptRequest, RegisterRecurringResponse,
    UpdateRecurringRequest, UpdateRecurringResponse,
};
use medousa_types::{RecurringListQuery, RecurringRunsQuery};
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::DaemonState;
use super::sdk::{client, sdk_error};

#[tauri::command]
pub async fn recurring_list(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    enabled_only: Option<bool>,
) -> Result<RecurringListResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_recurring_schedules_filtered(RecurringListQuery { enabled_only })
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .recurring()
        .list(&RecurringListQuery { enabled_only })
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn recurring_register_prompt(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    request: RegisterRecurringPromptRequest,
) -> Result<RegisterRecurringResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .register_prompt_schedule(request)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .recurring()
        .register_prompt(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn recurring_update(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    recurring_id: String,
    request: UpdateRecurringRequest,
) -> Result<UpdateRecurringResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .update_recurring_schedule(recurring_id.trim(), request)
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .recurring()
        .update(recurring_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn recurring_delete(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    recurring_id: String,
) -> Result<DeleteRecurringResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .delete_recurring_schedule(recurring_id.trim())
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .recurring()
        .delete(recurring_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn recurring_list_runs(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    recurring_id: String,
    limit: Option<usize>,
) -> Result<RecurringRunsResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .list_recurring_runs(recurring_id.trim(), RecurringRunsQuery { limit })
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .recurring()
        .runs(recurring_id.trim(), &RecurringRunsQuery { limit })
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn recurring_get_delivery(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    recurring_id: String,
) -> Result<RecurringDeliveryResponse, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .recurring_delivery(recurring_id.trim())
            .await
            .map_err(|error| error.to_string());
    }
    client(&state)?
        .recurring()
        .delivery_status(recurring_id.trim())
        .await
        .map_err(sdk_error)
}
