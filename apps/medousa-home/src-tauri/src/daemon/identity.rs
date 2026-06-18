use crate::daemon::types::{
    CreateUserProfileRequest, CreateUserProfileResponse, IdentityContextRequest,
    ListUserProfilesResponse, SetActiveUserProfileRequest, SetActiveUserProfileResponse,
};
use serde_json::Value;
use tauri::State;

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
) -> Result<ListUserProfilesResponse, String> {
    workshop_http::get_json(&state, "/v1/identity/profiles").await
}

#[tauri::command]
pub async fn identity_create_profile(
    state: State<'_, DaemonState>,
    slug: String,
    display_name: String,
) -> Result<CreateUserProfileResponse, String> {
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
    profile_id: String,
) -> Result<SetActiveUserProfileResponse, String> {
    workshop_http::put_json(
        &state,
        "/v1/identity/profiles/active",
        &SetActiveUserProfileRequest { profile_id },
    )
    .await
}
