use crate::daemon::types::WorkCardDetail;
use crate::daemon::DaemonState;
use reqwest::Client;
use tauri::State;

#[tauri::command]
pub async fn workspace_get_card(
    state: State<'_, DaemonState>,
    card_id: String,
) -> Result<WorkCardDetail, String> {
    let base = state
        .daemon_url
        .lock()
        .expect("daemon url lock")
        .clone();
    let encoded = urlencoding::encode(card_id.trim());
    let client = Client::new();
    let response = client
        .get(format!("{base}/v1/workspace/cards/{encoded}"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}
