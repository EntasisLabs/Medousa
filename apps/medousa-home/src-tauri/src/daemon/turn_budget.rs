use crate::daemon::types::{
    TurnBudgetApproveRequest, TurnBudgetDenyRequest, TurnBudgetRequestListResponse,
    TurnBudgetRequestResponse,
};
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

fn default_home_resolved_by() -> String {
    #[cfg(target_os = "ios")]
    {
        return "home-ios".to_string();
    }
    #[cfg(target_os = "android")]
    {
        return "home-android".to_string();
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        "home-desktop".to_string()
    }
}

#[tauri::command]
pub async fn turn_budget_approve(
    state: State<'_, DaemonState>,
    request_id: String,
    extra_rounds: Option<usize>,
    resolved_by: Option<String>,
) -> Result<TurnBudgetRequestResponse, String> {
    let body = TurnBudgetApproveRequest {
        extra_rounds,
        resolved_by: Some(
            resolved_by
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(default_home_resolved_by),
        ),
    };
    client(&state)
        .budget()
        .approve(request_id.trim(), &body)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn turn_budget_deny(
    state: State<'_, DaemonState>,
    request_id: String,
    resolved_by: Option<String>,
) -> Result<TurnBudgetRequestResponse, String> {
    let body = TurnBudgetDenyRequest {
        resolved_by: Some(
            resolved_by
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(default_home_resolved_by),
        ),
    };
    client(&state)
        .budget()
        .deny(request_id.trim(), &body)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn turn_budget_list(
    state: State<'_, DaemonState>,
    pending_only: Option<bool>,
) -> Result<TurnBudgetRequestListResponse, String> {
    client(&state)
        .budget()
        .list(pending_only.unwrap_or(true))
        .await
        .map_err(sdk_error)
}
