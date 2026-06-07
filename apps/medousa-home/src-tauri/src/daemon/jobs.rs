use crate::daemon::types::{
    ArchiveAskJobRequest, ArchiveAskJobResponse, AskJobCompleteActionsRequest,
    AskJobCompleteActionsResponse, EnqueueAskRequest, EnqueueResponse, JobResultResponse,
};
use reqwest::Client;
use tauri::State;

use super::DaemonState;

#[tauri::command]
pub async fn job_get_result(
    state: State<'_, DaemonState>,
    job_id: String,
) -> Result<JobResultResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let encoded = urlencoding::encode(job_id.trim());
    let client = Client::new();
    let response = client
        .get(format!("{base}/v1/jobs/{encoded}/result"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("job result failed ({status}): {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn job_enqueue_ask(
    state: State<'_, DaemonState>,
    prompt: String,
    model_hint: Option<String>,
    manuscript_id: Option<String>,
    additional_manuscript_ids: Option<Vec<String>>,
    suggested_capability_ids: Option<Vec<String>>,
) -> Result<EnqueueResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let request = EnqueueAskRequest {
        prompt,
        policy_profile: Some("interactive".to_string()),
        model_hint,
        max_turns: Some(1),
        identity_user_id: None,
        identity_persona_id: None,
        identity_channel_id: None,
        manuscript_id,
        additional_manuscript_ids,
        suggested_capability_ids,
    };
    let client = Client::new();
    let response = client
        .post(format!("{base}/v1/jobs/ask"))
        .json(&request)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("job ask failed ({status}): {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn job_complete_actions(
    state: State<'_, DaemonState>,
    job_id: String,
    write_journal_path: Option<String>,
    notify_channel: Option<String>,
) -> Result<AskJobCompleteActionsResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let encoded = urlencoding::encode(job_id.trim());
    let request = AskJobCompleteActionsRequest {
        write_journal_path,
        notify_channel,
    };
    let client = Client::new();
    let response = client
        .post(format!("{base}/v1/jobs/{encoded}/complete-actions"))
        .json(&request)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("ask complete-actions failed ({status}): {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn job_archive_ask(
    state: State<'_, DaemonState>,
    job_id: String,
    purge_output: Option<bool>,
) -> Result<ArchiveAskJobResponse, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let encoded = urlencoding::encode(job_id.trim());
    let request = ArchiveAskJobRequest {
        purge_output: purge_output.unwrap_or(true),
    };
    let client = Client::new();
    let response = client
        .post(format!("{base}/v1/jobs/{encoded}/archive"))
        .json(&request)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("ask archive failed ({status}): {body}"));
    }
    response.json().await.map_err(|err| err.to_string())
}
