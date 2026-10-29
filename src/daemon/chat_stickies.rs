//! Authenticated chat-local sticky board API; shares the exact agent storage path.
use crate::chat_stickies::StickyBoard;
use crate::chat_sticky_tools::{StickyMutateAction, store_for_runtime};
use crate::daemon::state::AppState;
use crate::request_principal::RequestPrincipal;
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};

type ApiError = (StatusCode, String);

async fn authorized_profile(
    principal: &RequestPrincipal,
    session_id: &str,
) -> Result<String, ApiError> {
    let profile = match principal.profile_id() {
        Some(profile) => profile.to_string(),
        None if matches!(
            principal.kind(),
            crate::request_principal::PrincipalKind::LocalApp
                | crate::request_principal::PrincipalKind::Root
        ) =>
        {
            crate::user_profiles::resolve_workshop_identity_user_id()
        }
        None => crate::request_principal::personal_portal_workshop_owner(
            principal,
            crate::shared_mode::is_shared_mode(),
        )
        .ok_or_else(|| {
            (
                StatusCode::FORBIDDEN,
                "authenticated profile required".into(),
            )
        })?,
    };
    let chat = session_id.to_string();
    let owner = profile.clone();
    let visible = tokio::task::spawn_blocking(move || {
        crate::session_catalog::session_visible_to_profile(&chat, &owner)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !visible {
        return Err((StatusCode::NOT_FOUND, "session is not available".into()));
    }
    Ok(profile)
}

fn storage_error(error: anyhow::Error) -> ApiError {
    let message = error.to_string();
    let status = if message.contains("revision conflict") || message.contains("request_id reused") {
        StatusCode::CONFLICT
    } else if message.contains("sticky") || message.contains("rollup") || message.contains("note") {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    (status, message)
}

pub async fn read(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(session_id): Path<String>,
) -> Result<Json<StickyBoard>, ApiError> {
    let profile = authorized_profile(&principal, &session_id).await?;
    let store = store_for_runtime(state.platform.composition())
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))?;
    store
        .read(&profile, &session_id)
        .await
        .map(Json)
        .map_err(storage_error)
}

pub async fn mutate(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(session_id): Path<String>,
    Json(action): Json<StickyMutateAction>,
) -> Result<Json<StickyBoard>, ApiError> {
    let profile = authorized_profile(&principal, &session_id).await?;
    let (_session, _lease) = crate::session_deletion::acquire_mutation_for_str(&session_id)
        .map_err(|e| (StatusCode::CONFLICT, e))?;
    let store = store_for_runtime(state.platform.composition())
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))?;
    store
        .mutate(&profile, &session_id, action.into_request())
        .await
        .map(Json)
        .map_err(storage_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_conflicts_are_not_success_or_validation_errors() {
        assert_eq!(
            storage_error(anyhow::anyhow!(
                "sticky revision conflict: expected 0, actual 1"
            ))
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            storage_error(anyhow::anyhow!(
                "sticky request_id reused with different payload"
            ))
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            storage_error(anyhow::anyhow!("rollup cannot replace pinned/exact note n")).0,
            StatusCode::BAD_REQUEST
        );
    }
    #[tokio::test]
    async fn unknown_chat_cannot_be_accessed() {
        let principal = RequestPrincipal::worker("sticky-test-other-profile");
        assert!(
            authorized_profile(&principal, "sticky-test-nonexistent-chat")
                .await
                .is_err()
        );
    }
}
