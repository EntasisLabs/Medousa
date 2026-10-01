//! Workshop HTTP surface for durable Bots.

use axum::Json;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use medousa_types::{
    BotId, BotListResponse, BotOpenResponse, BotProfile, CreateBotRequest, DuplicateBotRequest,
    SessionBotResponse, SetBotArchivedRequest, SetSessionBotRequest, UpdateBotRequest,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Deserialize)]
pub struct AskBotRequest {
    pub prompt: String,
    pub request_id: String,
    pub bot: Option<String>,
    pub agent: Option<String>,
    pub workshop: Option<String>,
}

fn ask_error(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
    retryable: bool,
) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({"error": {"code": code, "message": message.into(), "retryable": retryable}})),
    )
}

pub async fn ask_bot(
    State(state): State<crate::daemon::state::AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Json(request): Json<AskBotRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let profile = profile_id(&principal);
    if request.prompt.trim().is_empty()
        || request.request_id.trim().is_empty()
        || request.request_id.len() > 256
    {
        return Err(ask_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "prompt and request_id are required",
            false,
        ));
    }
    if request.bot.is_some() == request.agent.is_some() {
        return Err(ask_error(
            StatusCode::BAD_REQUEST,
            "invalid_selector",
            "choose --bot or --agent",
            false,
        ));
    }
    let runtime = match request.agent.as_deref() {
        Some("codex") => Some(medousa_types::coordination::ExternalPeerRuntime::Codex),
        Some(_) => {
            return Err(ask_error(
                StatusCode::BAD_REQUEST,
                "unknown_agent",
                "only enrolled Codex CLI Bots are supported",
                false,
            ));
        }
        None => None,
    };
    let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&json!({
        "prompt": request.prompt, "bot": request.bot, "agent": request.agent, "workshop": request.workshop,
    })).map_err(|error| ask_error(StatusCode::INTERNAL_SERVER_ERROR, "encode_failure", error.to_string(), false))?));
    let service = state.platform.delegation_service().ok_or_else(|| {
        ask_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "mesh_unavailable",
            "mesh delegation is unavailable",
            true,
        )
    })?;
    if let Some(ticket) = service
        .operator_bot_replay(&profile, &request.request_id, &fingerprint)
        .await
        .map_err(|error| {
            ask_error(
                StatusCode::CONFLICT,
                "request_conflict",
                error.to_string(),
                false,
            )
        })?
    {
        return Ok((
            StatusCode::ACCEPTED,
            Json(json!({"request_id": request.request_id, "ticket": ticket})),
        ));
    }
    let runtime_id = state
        .platform
        .agent()
        .worker_scheduler
        .execution_runtime_id();
    let mut targets = service
        .authorized_targets()
        .await
        .map_err(|error| {
            ask_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "workshop_unavailable",
                error.to_string(),
                true,
            )
        })?
        .into_iter()
        .map(|entry| {
            (
                entry.target.peer_device_id.clone(),
                entry.candidate.label,
                entry.target,
                entry.candidate.user_selectable,
            )
        })
        .collect::<Vec<_>>();
    targets.push((
        runtime_id.clone(),
        "This workshop".into(),
        crate::delegation::DelegationTarget {
            route_ref: "local".into(),
            peer_device_id: runtime_id.clone(),
            label: Some("This workshop".into()),
        },
        true,
    ));
    let selected_workshop = request
        .workshop
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let matching_targets = targets
        .iter()
        .filter(|entry| {
            selected_workshop
                .is_some_and(|value| entry.0 == value || entry.1.eq_ignore_ascii_case(value))
        })
        .collect::<Vec<_>>();
    if selected_workshop.is_some() && matching_targets.len() > 1 {
        return Err(ask_error(
            StatusCode::CONFLICT,
            "ambiguous_workshop",
            format!(
                "workshop candidates: {}",
                matching_targets
                    .iter()
                    .map(|entry| entry.0.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            false,
        ));
    }
    if let Some(selected) = selected_workshop.filter(|_| matching_targets.is_empty()) {
        let selected = selected.to_string();
        let workers = state.daemon_workers.clone();
        let paired = tokio::task::spawn_blocking(move || workers.list())
            .await
            .map_err(|error| {
                ask_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "pairing_store_failure",
                    error.to_string(),
                    false,
                )
            })?
            .map_err(|error| {
                ask_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "pairing_store_failure",
                    error.to_string(),
                    false,
                )
            })?;
        let known = paired.iter().any(|worker| {
            worker.workshop_device_id == selected || worker.label.eq_ignore_ascii_case(&selected)
        });
        return Err(if known {
            ask_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "workshop_offline",
                "paired workshop is unavailable",
                true,
            )
        } else {
            ask_error(
                StatusCode::FORBIDDEN,
                "unpaired_workshop",
                "workshop is not paired",
                false,
            )
        });
    }
    let workshop_id = matching_targets.first().map(|entry| entry.0.clone());
    let bot_selector = request.bot.clone();
    let bot_profile = profile.clone();
    let filter_workshop = if bot_selector.is_some() {
        None
    } else {
        workshop_id
    };
    let bot = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default().resolve_external(
            &bot_profile,
            bot_selector.as_deref(),
            runtime,
            filter_workshop.as_deref(),
        )
    })
    .await
    .map_err(|error| {
        ask_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "bot_store_failure",
            error.to_string(),
            false,
        )
    })?
    .map_err(|error| {
        let code = if error.starts_with("ambiguous") {
            "ambiguous_bot"
        } else {
            "unknown_bot"
        };
        ask_error(
            if code == "ambiguous_bot" {
                StatusCode::CONFLICT
            } else {
                StatusCode::NOT_FOUND
            },
            code,
            error,
            false,
        )
    })?;
    let home = bot
        .external_agent
        .as_ref()
        .expect("resolved external Bot")
        .home_workshop_id
        .as_str();
    if selected_workshop.is_some()
        && matching_targets
            .first()
            .is_some_and(|entry| entry.0 != home)
    {
        return Err(ask_error(
            StatusCode::CONFLICT,
            "workshop_denied",
            "Bot is pinned to a different home workshop",
            false,
        ));
    }
    let target = if let Some(entry) = targets.into_iter().find(|entry| entry.0 == home) {
        if !entry.3 {
            return Err(ask_error(
                StatusCode::FORBIDDEN,
                "permission_denied",
                "Bot home workshop does not allow explicit Assistant work",
                false,
            ));
        }
        entry.2
    } else {
        let workers = state.daemon_workers.clone();
        let paired = tokio::task::spawn_blocking(move || workers.list())
            .await
            .map_err(|error| {
                ask_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "pairing_store_failure",
                    error.to_string(),
                    false,
                )
            })?
            .map_err(|error| {
                ask_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "pairing_store_failure",
                    error.to_string(),
                    false,
                )
            })?;
        if paired
            .iter()
            .any(|worker| worker.workshop_device_id == home)
        {
            return Err(ask_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "workshop_offline",
                format!("Bot home workshop '{home}' is paired but unavailable"),
                true,
            ));
        }
        return Err(ask_error(
            StatusCode::FORBIDDEN,
            "unpaired_workshop",
            format!("Bot home workshop '{home}' is not paired"),
            false,
        ));
    };
    let ticket = service
        .submit_operator_bot(
            &profile,
            &request.request_id,
            &fingerprint,
            &request.prompt,
            &bot,
            target,
            &runtime_id,
        )
        .await
        .map_err(|error| {
            ask_error(
                StatusCode::CONFLICT,
                "admission_failed",
                error.to_string(),
                false,
            )
        })?;
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({"request_id": request.request_id, "ticket": ticket})),
    ))
}

pub async fn ask_bot_status(
    State(state): State<crate::daemon::state::AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let service = state.platform.delegation_service().ok_or_else(|| {
        ask_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "mesh_unavailable",
            "mesh delegation is unavailable",
            true,
        )
    })?;
    let status = service
        .operator_bot_status(&profile_id(&principal), &job_id)
        .await
        .map_err(|error| {
            ask_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "observation_failed",
                error.to_string(),
                true,
            )
        })?
        .ok_or_else(|| ask_error(StatusCode::NOT_FOUND, "unknown_job", "job not found", false))?;
    Ok(Json(status))
}

pub async fn cancel_ask_bot(
    State(state): State<crate::daemon::state::AppState>,
    Extension(principal): Extension<crate::request_principal::RequestPrincipal>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let service = state.platform.delegation_service().ok_or_else(|| {
        ask_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "mesh_unavailable",
            "mesh delegation is unavailable",
            true,
        )
    })?;
    let cancelled = service
        .cancel_operator_bot(&profile_id(&principal), &job_id)
        .await
        .map_err(|error| {
            ask_error(
                StatusCode::CONFLICT,
                "cancel_failed",
                error.to_string(),
                false,
            )
        })?;
    Ok(Json(cancelled))
}

fn profile_id(principal: &crate::request_principal::RequestPrincipal) -> String {
    principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id)
}

fn require_external_agent_admin(
    principal: &crate::request_principal::RequestPrincipal,
) -> Result<(), (StatusCode, String)> {
    if principal
        .capabilities()
        .contains(crate::request_principal::Capability::AdminExecute)
    {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "configuring an external-agent Bot requires execution administration".to_string(),
        ))
    }
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
    if request.external_agent.is_some() {
        require_external_agent_admin(&principal)?;
    }
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
    if request.external_agent.is_some() || request.clear_external_agent {
        require_external_agent_admin(&principal)?;
    }
    validate_manuscripts(
        &request.primary_manuscript_id,
        &request.additional_manuscript_ids,
    )?;
    let bot_id = parse_bot_id(&bot_id)?;
    let profile_id = profile_id(&principal);
    let bot = tokio::task::spawn_blocking(move || {
        crate::bot_profiles::BotProfileStore::daemon_default().update(&profile_id, &bot_id, request)
    })
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
    .map_err(store_error)?;
    if let Some(session_id) = bot.primary_session_id.as_deref()
        && let Err(error) = crate::session::set_session_display_name(session_id, &bot.display_name)
    {
        eprintln!("[medousa] Bot conversation title sync failed: {error}");
    }
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
