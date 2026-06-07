use crate::daemon::types::{ArtifactCommandRequest, ArtifactCommandResponse};
use reqwest::Client;
use tauri::State;

use super::DaemonState;

#[tauri::command]
pub async fn artifact_command(
    state: State<'_, DaemonState>,
    request: ArtifactCommandRequest,
) -> Result<ArtifactCommandResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::new();
    let response = client
        .post(format!("{base}/v1/runtime/artifact/command"))
        .json(&request)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("artifact command failed ({status}): {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}
