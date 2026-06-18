use crate::daemon::types::{
    TurnBudgetApproveRequest, TurnBudgetDenyRequest, TurnBudgetRequestListResponse,
    TurnBudgetRequestResponse,
};
use tauri::State;

use super::workshop_http;
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
    let encoded = urlencoding::encode(request_id.trim());
    let body = TurnBudgetApproveRequest {
        extra_rounds,
        resolved_by: Some(
            resolved_by
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(default_home_resolved_by),
        ),
    };
    workshop_http::post_json(
        &state,
        &format!("/v1/turns/budget-requests/{encoded}/approve"),
        &body,
    )
    .await
}

#[tauri::command]
pub async fn turn_budget_deny(
    state: State<'_, DaemonState>,
    request_id: String,
    resolved_by: Option<String>,
) -> Result<TurnBudgetRequestResponse, String> {
    let encoded = urlencoding::encode(request_id.trim());
    let body = TurnBudgetDenyRequest {
        resolved_by: Some(
            resolved_by
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(default_home_resolved_by),
        ),
    };
    workshop_http::post_json(
        &state,
        &format!("/v1/turns/budget-requests/{encoded}/deny"),
        &body,
    )
    .await
}

#[tauri::command]
pub async fn turn_budget_list(
    state: State<'_, DaemonState>,
    pending_only: Option<bool>,
) -> Result<TurnBudgetRequestListResponse, String> {
    let path = if pending_only.unwrap_or(true) {
        "/v1/turns/budget-requests?status=pending&limit=20".to_string()
    } else {
        "/v1/turns/budget-requests?limit=20".to_string()
    };
    workshop_http::get_json(&state, &path).await
}
