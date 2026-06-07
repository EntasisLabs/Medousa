use crate::daemon::types::{
    DeleteRecurringResponse, RecurringListResponse, RegisterRecurringPromptRequest,
    RegisterRecurringResponse, UpdateRecurringRequest, UpdateRecurringResponse,
};
use reqwest::Client;
use tauri::State;

use super::DaemonState;

#[tauri::command]
pub async fn recurring_list(
    state: State<'_, DaemonState>,
    enabled_only: Option<bool>,
) -> Result<RecurringListResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let mut url = format!("{base}/v1/recurring");
    if let Some(value) = enabled_only {
        url.push_str(&format!("?enabled_only={value}"));
    }

    let client = Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("recurring list failed ({status}): {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn recurring_register_prompt(
    state: State<'_, DaemonState>,
    request: RegisterRecurringPromptRequest,
) -> Result<RegisterRecurringResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::new();
    let response = client
        .post(format!("{base}/v1/recurring/prompt"))
        .json(&request)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("recurring register failed ({status}): {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn recurring_update(
    state: State<'_, DaemonState>,
    recurring_id: String,
    request: UpdateRecurringRequest,
) -> Result<UpdateRecurringResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::new();
    let response = client
        .patch(format!("{base}/v1/recurring/{}", recurring_id.trim()))
        .json(&request)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("recurring update failed ({status}): {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn recurring_delete(
    state: State<'_, DaemonState>,
    recurring_id: String,
) -> Result<DeleteRecurringResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::new();
    let response = client
        .delete(format!("{base}/v1/recurring/{}", recurring_id.trim()))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("recurring delete failed ({status}): {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}
