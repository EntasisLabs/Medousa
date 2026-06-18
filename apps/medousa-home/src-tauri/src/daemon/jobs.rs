use crate::daemon::types::{
    ArchiveAskJobRequest, ArchiveAskJobResponse, AskJobCompleteActionsRequest,
    AskJobCompleteActionsResponse, EnqueueAskRequest, EnqueueResponse, JobResultResponse,
};
use tauri::State;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn job_get_result(
    state: State<'_, DaemonState>,
    job_id: String,
) -> Result<JobResultResponse, String> {
    let encoded = urlencoding::encode(job_id.trim());
    workshop_http::get_json(&state, &format!("/v1/jobs/{encoded}/result")).await
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
    workshop_http::post_json(&state, "/v1/jobs/ask", &request).await
}

#[tauri::command]
pub async fn job_complete_actions(
    state: State<'_, DaemonState>,
    job_id: String,
    write_journal_path: Option<String>,
    notify_channel: Option<String>,
) -> Result<AskJobCompleteActionsResponse, String> {
    let encoded = urlencoding::encode(job_id.trim());
    let request = AskJobCompleteActionsRequest {
        write_journal_path,
        notify_channel,
    };
    workshop_http::post_json(
        &state,
        &format!("/v1/jobs/{encoded}/complete-actions"),
        &request,
    )
    .await
}

#[tauri::command]
pub async fn job_archive_ask(
    state: State<'_, DaemonState>,
    job_id: String,
    purge_output: Option<bool>,
) -> Result<ArchiveAskJobResponse, String> {
    let encoded = urlencoding::encode(job_id.trim());
    let request = ArchiveAskJobRequest {
        purge_output: purge_output.unwrap_or(true),
    };
    workshop_http::post_json(&state, &format!("/v1/jobs/{encoded}/archive"), &request).await
}
