use crate::daemon::types::{
    TurnBudgetApproveRequest, TurnBudgetDenyRequest, TurnBudgetRequestResponse,
};
use reqwest::Client;
use tauri::State;

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
    let base = state.daemon_url.lock().map_err(|_| "daemon url lock poisoned")?.clone();
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
    resolved_by: Option<String>,
) -> Result<TurnBudgetRequestResponse, String> {
    let base = state.daemon_url.lock().map_err(|_| "daemon url lock poisoned")?.clone();
    let encoded = urlencoding::encode(request_id.trim());
    let body = TurnBudgetDenyRequest {
        resolved_by: Some(
            resolved_by
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(default_home_resolved_by),
        ),
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
