use crate::daemon::types::{
    TurnBudgetApproveRequest, TurnBudgetDenyRequest, TurnBudgetRequestResponse,
};
use reqwest::Client;
use tauri::State;

use super::DaemonState;

#[tauri::command]
pub async fn turn_budget_approve(
    state: State<'_, DaemonState>,
    request_id: String,
    extra_rounds: Option<usize>,
) -> Result<TurnBudgetRequestResponse, String> {
    let base = state.daemon_url.lock().map_err(|_| "daemon url lock poisoned")?.clone();
    let encoded = urlencoding::encode(request_id.trim());
    let body = TurnBudgetApproveRequest {
        extra_rounds,
        resolved_by: Some("medousa-home".to_string()),
    };
    Client::new()
        .post(format!("{base}/v1/turns/budget-requests/{encoded}/approve"))
        .json(&body)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn turn_budget_deny(
    state: State<'_, DaemonState>,
    request_id: String,
) -> Result<TurnBudgetRequestResponse, String> {
    let base = state.daemon_url.lock().map_err(|_| "daemon url lock poisoned")?.clone();
    let encoded = urlencoding::encode(request_id.trim());
    let body = TurnBudgetDenyRequest {
        resolved_by: Some("medousa-home".to_string()),
    };
    Client::new()
        .post(format!("{base}/v1/turns/budget-requests/{encoded}/deny"))
        .json(&body)
        .send()
        .await
        .map_err(|err| err.to_string())?
        .error_for_status()
        .map_err(|err| err.to_string())?
        .json()
        .await
        .map_err(|err| err.to_string())
}
