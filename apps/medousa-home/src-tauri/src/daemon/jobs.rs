use crate::daemon::types::{
    ArchiveAskJobRequest, ArchiveAskJobResponse, AskJobCompleteActionsRequest,
    AskJobCompleteActionsResponse, EnqueueAskRequest, EnqueueResponse, JobResultResponse,
};
use tauri::State;

use super::sdk::{client, sdk_error};
use super::DaemonState;

#[tauri::command]
pub async fn job_get_result(
    state: State<'_, DaemonState>,
    job_id: String,
) -> Result<JobResultResponse, String> {
    client(&state)
        .jobs()
        .result(job_id.trim())
        .await
        .map_err(sdk_error)
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
    client(&state)
        .jobs()
        .enqueue_ask(&request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn job_complete_actions(
    state: State<'_, DaemonState>,
    job_id: String,
    write_journal_path: Option<String>,
    notify_channel: Option<String>,
) -> Result<AskJobCompleteActionsResponse, String> {
    let request = AskJobCompleteActionsRequest {
        write_journal_path,
        notify_channel,
    };
    client(&state)
        .jobs()
        .complete_actions(job_id.trim(), &request)
        .await
        .map_err(sdk_error)
}

#[tauri::command]
pub async fn job_archive_ask(
    state: State<'_, DaemonState>,
    job_id: String,
    purge_output: Option<bool>,
) -> Result<ArchiveAskJobResponse, String> {
    let request = ArchiveAskJobRequest {
        purge_output: purge_output.unwrap_or(true),
    };
    client(&state)
        .jobs()
        .archive(job_id.trim(), &request)
        .await
        .map_err(sdk_error)
}
