use crate::daemon::types::{
    CreateUserProfileRequest, CreateUserProfileResponse, ExportUserProfileRequest,
    ExportUserProfileResponse, IdentityContextRequest, IdentityDigestPreviewResponse,
    IdentityExportMarkdownRequest, IdentityExportMarkdownResponse, IdentityRememberRequest,
    IdentityRememberResponse, ImportUserProfileRequest, ImportUserProfileResponse,
    ListUserProfilesResponse, SetActiveUserProfileRequest, SetActiveUserProfileResponse,
};
use serde_json::Value;
use tauri::State;

use crate::embedded_daemon::EmbeddedDaemonState;

use super::workshop_http;
use super::DaemonState;

#[tauri::command]
pub async fn identity_get_context(
    state: State<'_, DaemonState>,
    request: IdentityContextRequest,
) -> Result<Value, String> {
    workshop_http::post_json(&state, "/v1/identity/context", &request).await
}

#[tauri::command]
pub async fn identity_list_profiles(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
) -> Result<ListUserProfilesResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client.list_profiles().map_err(|error| error.to_string());
    }
    workshop_http::get_json(&state, "/v1/identity/profiles").await
}

#[tauri::command]
pub async fn identity_create_profile(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    slug: String,
    display_name: String,
) -> Result<CreateUserProfileResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .create_profile(&slug, &display_name)
            .map_err(|error| error.to_string());
    }
    workshop_http::post_json(
        &state,
        "/v1/identity/profiles",
        &CreateUserProfileRequest { slug, display_name },
    )
    .await
}

#[tauri::command]
pub async fn identity_set_active_profile(
    state: State<'_, DaemonState>,
    _embedded_state: State<'_, EmbeddedDaemonState>,
    profile_id: String,
) -> Result<SetActiveUserProfileResponse, String> {
    #[cfg(target_os = "ios")]
    if let Some(client) = _embedded_state.client_if_active().await? {
        return client
            .set_active_profile(&profile_id)
            .map_err(|error| error.to_string());
    }
    workshop_http::put_json(
        &state,
        "/v1/identity/profiles/active",
        &SetActiveUserProfileRequest { profile_id },
    )
    .await
}

#[tauri::command]
pub async fn identity_remember(
    state: State<'_, DaemonState>,
    request: IdentityRememberRequest,
) -> Result<IdentityRememberResponse, String> {
    workshop_http::post_json(&state, "/v1/identity/remember", &request).await
}

#[tauri::command]
pub async fn identity_digest_preview(
    state: State<'_, DaemonState>,
    request: IdentityContextRequest,
) -> Result<IdentityDigestPreviewResponse, String> {
    workshop_http::post_json(&state, "/v1/identity/digest-preview", &request).await
}

#[tauri::command]
pub async fn identity_export_markdown(
    state: State<'_, DaemonState>,
    request: IdentityExportMarkdownRequest,
) -> Result<IdentityExportMarkdownResponse, String> {
    workshop_http::post_json(&state, "/v1/identity/export-markdown", &request).await
}

#[tauri::command]
pub async fn identity_export_profile(
    state: State<'_, DaemonState>,
    profile_id: String,
    session_limit: Option<usize>,
    node_limit_per_session: Option<usize>,
) -> Result<ExportUserProfileResponse, String> {
    workshop_http::post_json(
        &state,
        "/v1/identity/profiles/export",
        &ExportUserProfileRequest {
            profile_id,
            session_limit: session_limit.unwrap_or(500),
            node_limit_per_session: node_limit_per_session.unwrap_or(500),
        },
    )
    .await
}

#[tauri::command]
pub async fn identity_import_profile(
    state: State<'_, DaemonState>,
    bundle: Value,
    dry_run: Option<bool>,
) -> Result<ImportUserProfileResponse, String> {
    workshop_http::post_json(
        &state,
        "/v1/identity/profiles/import",
        &ImportUserProfileRequest {
            bundle: serde_json::from_value(bundle)
                .map_err(|err| format!("invalid profile bundle: {err}"))?,
            dry_run: dry_run.unwrap_or(false),
        },
    )
    .await
}
