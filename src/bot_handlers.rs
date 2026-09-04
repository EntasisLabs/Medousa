//! Workshop HTTP surface for durable Bots.

use axum::Json;
use axum::extract::{Extension, Path};
use axum::http::StatusCode;
use medousa_types::{
    BotId, BotListResponse, BotOpenResponse, BotProfile, CreateBotRequest, DuplicateBotRequest,
    SessionBotResponse, SetBotArchivedRequest, SetSessionBotRequest, UpdateBotRequest,
};

fn profile_id(principal: &crate::request_principal::RequestPrincipal) -> String {
    principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id)
}

fn parse_bot_id(value: &str) -> Result<BotId, (StatusCode, String)> {
    BotId::parse(value).map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
}

fn store_error(error: String) -> (StatusCode, String) {
    let lower = error.to_ascii_lowercase();
    let status = if lower.contains("not found") {
        StatusCode::NOT_FOUND
    } else if lower.contains("conflict")
        || lower.contains("already bound")
        || lower.contains("already has")
    {
        StatusCode::CONFLICT
    } else if lower.contains("read bot")
        || lower.contains("write bot")
        || lower.contains("decode bot")
        || lower.contains("encode bot")
        || lower.contains("lock poisoned")
    {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, error)
}

fn validate_manuscripts(primary: &str, additional: &[String]) -> Result<(), (StatusCode, String)> {
    crate::identity_manuscript::build_manuscript_context(primary).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("primary Specialist is not available: {error}"),
        )
    })?;
    for manuscript_id in additional {
        crate::identity_manuscript::build_manuscript_context(manuscript_id).map_err(|error| {
            (
                StatusCode::BAD_REQUEST,
                format!("additional Specialist '{manuscript_id}' is not available: {error}"),
            )
        })?;
    }
    Ok(())
}

fn ensure_visible_session(session_id: &str, profile_id: &str) -> Result<(), (StatusCode, String)> {
    crate::session_storage::SessionId::parse(session_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    if !crate::session_catalog::session_visible_to_profile(session_id, profile_id) {
        return Err((StatusCode::NOT_FOUND, "conversation not found".to_string()));
    }
    Ok(())
}

fn ensure_bot_session(response: &BotOpenResponse, profile_id: &str) -> Result<(), String> {
    crate::session_catalog::ensure_named_session_for_profile(
        &response.binding.session_id,
        Some(response.bot.display_name.clone()),
        profile_id,
    )
}

pub async fn list_bots(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
) -> Result<Json<BotListResponse>, (StatusCode, String)> {
    let profile_id = profile_id(&principal);
    let bots = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default().list(&profile_id)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    Ok(Json(BotListResponse { bots }))
}

pub async fn create_bot(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Json(request): Json<CreateBotRequest>,
) -> Result<(StatusCode, Json<BotOpenResponse>), (StatusCode, String)> {
    validate_manuscripts(
        &request.primary_manuscript_id,
        &request.additional_manuscript_ids,
    )?;
    let profile_id = profile_id(&principal);
    let store_profile_id = profile_id.clone();
    let session_id = crate::session_storage::new_session_id().to_string();
    let response = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default().create(
            &store_profile_id,
            &session_id,
            request,
        )
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    ensure_bot_session(&response, &profile_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn get_bot(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(bot_id): Path<String>,
) -> Result<Json<BotProfile>, (StatusCode, String)> {
    let bot_id = parse_bot_id(&bot_id)?;
    let profile_id = profile_id(&principal);
    let bot = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default().get(&profile_id, &bot_id)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    Ok(Json(bot))
}

pub async fn update_bot(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(bot_id): Path<String>,
    Json(request): Json<UpdateBotRequest>,
) -> Result<Json<BotProfile>, (StatusCode, String)> {
    validate_manuscripts(
        &request.primary_manuscript_id,
        &request.additional_manuscript_ids,
    )?;
    let bot_id = parse_bot_id(&bot_id)?;
    let profile_id = profile_id(&principal);
    let bot = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default().update(
            &profile_id,
            &bot_id,
            request,
        )
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    Ok(Json(bot))
}

pub async fn set_bot_archived(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(bot_id): Path<String>,
    Json(request): Json<SetBotArchivedRequest>,
) -> Result<Json<BotProfile>, (StatusCode, String)> {
    let bot_id = parse_bot_id(&bot_id)?;
    let profile_id = profile_id(&principal);
    let bot = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default().set_archived(
            &profile_id,
            &bot_id,
            request,
        )
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    Ok(Json(bot))
}

pub async fn duplicate_bot(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(bot_id): Path<String>,
    Json(request): Json<DuplicateBotRequest>,
) -> Result<(StatusCode, Json<BotOpenResponse>), (StatusCode, String)> {
    let bot_id = parse_bot_id(&bot_id)?;
    let profile_id = profile_id(&principal);
    let store_profile_id = profile_id.clone();
    let session_id = crate::session_storage::new_session_id().to_string();
    let response = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default().duplicate(
            &store_profile_id,
            &bot_id,
            &session_id,
            request,
        )
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    ensure_bot_session(&response, &profile_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok((StatusCode::CREATED, Json(response)))
}

pub async fn open_bot(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(bot_id): Path<String>,
) -> Result<Json<BotOpenResponse>, (StatusCode, String)> {
    let bot_id = parse_bot_id(&bot_id)?;
    let profile_id = profile_id(&principal);
    let store_profile_id = profile_id.clone();
    let replacement_session_id = crate::session_storage::new_session_id().to_string();
    let response = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default().open(
            &store_profile_id,
            &bot_id,
            &replacement_session_id,
        )
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    ensure_bot_session(&response, &profile_id)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(response))
}

pub async fn get_session_bot(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionBotResponse>, (StatusCode, String)> {
    let profile_id = profile_id(&principal);
    ensure_visible_session(&session_id, &profile_id)?;
    let response = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default()
            .resolve_session(&profile_id, &session_id)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    Ok(Json(response))
}

pub async fn bind_session_bot(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(session_id): Path<String>,
    Json(request): Json<SetSessionBotRequest>,
) -> Result<Json<SessionBotResponse>, (StatusCode, String)> {
    let profile_id = profile_id(&principal);
    ensure_visible_session(&session_id, &profile_id)?;
    let response = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default().bind_session(
            &profile_id,
            &session_id,
            request,
        )
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    Ok(Json(response))
}

pub async fn unbind_session_bot(
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionBotResponse>, (StatusCode, String)> {
    let profile_id = profile_id(&principal);
    ensure_visible_session(&session_id, &profile_id)?;
    let response = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default()
            .unbind_session(&profile_id, &session_id)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    Ok(Json(response))
}
