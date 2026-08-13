use std::sync::Arc;

use axum::extract::{ConnectInfo, Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{delete, get, post},
};
use serde::Deserialize;

use crate::pairing::{
    PairHeartbeatRequest, PairInitRequest, PairVerifyRequest, PairingService,
    RevokePairingAuthority, RevokePairingResult,
};
use crate::request_principal::{PrincipalKind, RequestPrincipal};

#[derive(Clone)]
pub struct PairingApiState {
    pub service: Arc<PairingService>,
}

/// Anonymous pairing ceremony routes. H01 will add the active-window and
/// admission limits; no administrative/readback route belongs here.
pub fn bootstrap_routes() -> Router<PairingApiState> {
    Router::new()
        .route("/pair/init", post(pair_init))
        .route("/pair/verify", post(pair_verify))
}

/// Pairing administration and authenticated peer lifecycle routes.
pub fn protected_routes() -> Router<PairingApiState> {
    Router::new()
        .route("/pair/status", get(pair_status))
        .route("/pair/iroh-ticket", get(get_iroh_ticket))
        .route("/qr", get(get_qr))
        .route("/qr/rotate", post(rotate_qr))
        .route("/qr/image", get(get_qr_image))
        .route("/qr.png", get(get_qr_png))
        .route("/pair/code", get(get_pair_code))
        .route(
            "/pair/heartbeat",
            get(pair_heartbeat).post(pair_heartbeat_post),
        )
        .route("/pair/{pairing_id}", delete(revoke_pairing))
}

async fn pair_status(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.service.pair_status().await {
        Ok(response) => Ok(Json(serde_json::to_value(response).unwrap_or_default())),
        Err(err) => {
            eprintln!("medousa-daemon: GET /pair/status failed: {err:#}");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_iroh_ticket(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match state.service.iroh_ticket() {
        Some(response) => Ok(Json(serde_json::to_value(response).unwrap_or_default())),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QrQuery {
    /// When true, embed Iroh ticket (v2). Default is compact v1 for camera/Messages.
    #[serde(default)]
    full: Option<bool>,
    /// Shared-mode seat invite: bind the pairing to this profile id.
    #[serde(default)]
    profile_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RotateQrBody {
    #[serde(default)]
    profile_id: Option<String>,
}

fn wants_full_qr(query: &QrQuery) -> bool {
    query.full.unwrap_or(false)
}

async fn get_qr(
    State(state): State<PairingApiState>,
    Query(query): Query<QrQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .service
        .current_qr_with_options(wants_full_qr(&query))
        .await
        .map(|response| Json(serde_json::to_value(response).unwrap_or_default()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn rotate_qr(
    State(state): State<PairingApiState>,
    Query(query): Query<QrQuery>,
    body: Option<Json<RotateQrBody>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let profile_id = body
        .and_then(|Json(body)| body.profile_id)
        .or(query.profile_id);
    state
        .service
        .rotate_qr_for_profile(profile_id.as_deref())
        .await
        .map(|response| Json(serde_json::to_value(response).unwrap_or_default()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_qr_png(
    State(state): State<PairingApiState>,
    Query(query): Query<QrQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let qr = state
        .service
        .current_qr_with_options(wants_full_qr(&query))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let png = state
        .service
        .render_qr_png(&qr.url)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(([(axum::http::header::CONTENT_TYPE, "image/png")], png))
}

async fn get_qr_image(
    State(state): State<PairingApiState>,
    Query(query): Query<QrQuery>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .service
        .current_qr_image_with_options(wants_full_qr(&query))
        .await
        .map(|response| Json(serde_json::to_value(response).unwrap_or_default()))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn get_pair_code(
    State(state): State<PairingApiState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    state
        .service
        .current_short_code()
        .await
        .map(|short_code| Json(serde_json::json!({ "shortCode": short_code })))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn pair_init(
    State(state): State<PairingApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Json(body): Json<PairInitRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let source_ip = addr.ip().to_string();
    let response = state
        .service
        .pair_init(body, &source_ip)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status = if response.status == "challenge" {
        StatusCode::OK
    } else if response.reason.as_deref() == Some("token_already_used") {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_REQUEST
    };
    Ok((
        status,
        Json(serde_json::to_value(response).unwrap_or_default()),
    ))
}

async fn pair_verify(
    State(state): State<PairingApiState>,
    Json(body): Json<PairVerifyRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    match state.service.pair_verify(body).await {
        Ok(response) => {
            let status = if response.status == "paired" {
                StatusCode::OK
            } else {
                StatusCode::BAD_REQUEST
            };
            Ok((
                status,
                Json(serde_json::to_value(response).unwrap_or_default()),
            ))
        }
        Err(_) => Err(StatusCode::BAD_REQUEST),
    }
}

async fn pair_heartbeat(
    State(state): State<PairingApiState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    run_pair_heartbeat(&state, &principal, None).await
}

async fn pair_heartbeat_post(
    State(state): State<PairingApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(body): Json<PairHeartbeatRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    run_pair_heartbeat(&state, &principal, Some(body)).await
}

async fn run_pair_heartbeat(
    state: &PairingApiState,
    principal: &RequestPrincipal,
    body: Option<PairHeartbeatRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let response = state
        .service
        .pair_heartbeat(principal.credential_id().map(|id| id.as_str()), body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let status = if response.status == "ok" {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    };
    Ok((
        status,
        Json(serde_json::to_value(response).unwrap_or_default()),
    ))
}

async fn revoke_pairing(
    State(state): State<PairingApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(pairing_id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let authority = match principal.kind() {
        PrincipalKind::LegacyLocal | PrincipalKind::Root => RevokePairingAuthority::Administrator,
        _ => principal
            .credential_id()
            .map(|id| RevokePairingAuthority::Credential(id.as_str()))
            .unwrap_or(RevokePairingAuthority::Unauthenticated),
    };
    match state.service.revoke_pairing(&pairing_id, authority).await {
        Ok(RevokePairingResult::Removed) => Ok(StatusCode::NO_CONTENT),
        Ok(RevokePairingResult::NotFound) => Err(StatusCode::NOT_FOUND),
        Ok(RevokePairingResult::Unauthorized) => Err(StatusCode::UNAUTHORIZED),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
