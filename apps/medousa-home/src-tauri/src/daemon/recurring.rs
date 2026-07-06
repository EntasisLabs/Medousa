use crate::daemon::types::{
    DeleteRecurringResponse, RecurringDeliveryResponse, RecurringListResponse,
    RecurringRunsResponse, RegisterRecurringPromptRequest, RegisterRecurringResponse,
    UpdateRecurringRequest, UpdateRecurringResponse,
};
use medousa_types::{RecurringListQuery, RecurringRunsQuery};
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

#[tauri::command]
pub async fn recurring_list(
    state: State<'_, DaemonState>,
    enabled_only: Option<bool>,
) -> Result<RecurringListResponse, String> {
    client(&state)
        .recurring()
        .list(&RecurringListQuery { enabled_only })
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn recurring_register_prompt(
    state: State<'_, DaemonState>,
    request: RegisterRecurringPromptRequest,
) -> Result<RegisterRecurringResponse, String> {
    client(&state)
        .recurring()
        .register_prompt(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn recurring_update(
    state: State<'_, DaemonState>,
    recurring_id: String,
    request: UpdateRecurringRequest,
) -> Result<UpdateRecurringResponse, String> {
    client(&state)
        .recurring()
        .update(recurring_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn recurring_delete(
    state: State<'_, DaemonState>,
    recurring_id: String,
) -> Result<DeleteRecurringResponse, String> {
    client(&state)
        .recurring()
        .delete(recurring_id.trim())
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn recurring_list_runs(
    state: State<'_, DaemonState>,
    recurring_id: String,
    limit: Option<usize>,
) -> Result<RecurringRunsResponse, String> {
    client(&state)
        .recurring()
        .runs(
            recurring_id.trim(),
            &RecurringRunsQuery { limit },
        )
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn recurring_get_delivery(
    state: State<'_, DaemonState>,
    recurring_id: String,
) -> Result<RecurringDeliveryResponse, String> {
    client(&state)
        .recurring()
        .delivery_status(recurring_id.trim())
        .await
        .map_err(sdk_error)
}
