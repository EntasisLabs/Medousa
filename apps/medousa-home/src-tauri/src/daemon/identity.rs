use crate::daemon::types::IdentityContextRequest;
use reqwest::Client;
use serde_json::Value;
use tauri::State;

use super::DaemonState;

#[tauri::command]
pub async fn identity_get_context(
    state: State<'_, DaemonState>,
    request: IdentityContextRequest,
) -> Result<Value, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::new();
    let response = client
        .post(format!("{base}/v1/identity/context"))
        .json(&request)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("identity context failed ({status}): {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}
