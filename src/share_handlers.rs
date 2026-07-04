//! HTTP handlers for workshop sharing (`/v1/share/*`).

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::environment_store::environment_hub;
use crate::pairing::PairingService;
use crate::share::bundle::{
    ShareBundle, ShareCapabilitiesResponse, ShareExportRequest, ShareImportRequest,
    ShareImportResult,
};
use crate::share::service::{export_bundle, import_bundle};

#[derive(Clone)]
pub struct ShareApiState {
    pub pairing: Option<Arc<PairingService>>,
    pub local_device_id: String,
    pub local_peer_name: String,
}

pub fn share_router(state: ShareApiState) -> Router {
    Router::new()
        .route("/v1/share/capabilities", get(share_capabilities))
        .route("/v1/share/export", post(share_export))
        .route("/v1/share/import", post(share_import))
        .route("/v1/share/push", post(share_push))
        .with_state(state)
}

async fn share_capabilities() -> Json<ShareCapabilitiesResponse> {
    Json(ShareCapabilitiesResponse::current())
}

async fn share_export(
    State(state): State<ShareApiState>,
    Json(body): Json<ShareExportRequest>,
) -> Result<Json<ShareBundle>, (StatusCode, String)> {
    let source = crate::share::bundle::ShareSourceWorkshop {
        device_id: state.local_device_id.clone(),
        name: state.local_peer_name.clone(),
    };
    export_bundle(body, source).map(Json).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            err.to_string(),
        )
    })
}

async fn share_import(
    State(state): State<ShareApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<ShareImportRequest>,
) -> Result<Json<ShareImportResult>, (StatusCode, String)> {
    if !is_local_request(addr.ip()) {
        authorize_remote_share(&state, &headers).await?;
    }
    let errors = body.bundle.validate();
    if !errors.is_empty() {
        return Err((StatusCode::BAD_REQUEST, errors.join("; ")));
    }
    import_bundle(environment_hub(), body)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))
}

async fn share_push(
    State(state): State<ShareApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<ShareImportRequest>,
) -> Result<Json<ShareImportResult>, (StatusCode, String)> {
    if !is_local_request(addr.ip()) {
        authorize_remote_share(&state, &headers).await?;
    }
    share_import(
        State(state),
        ConnectInfo(addr),
        headers,
        Json(body),
    )
    .await
}

async fn authorize_remote_share(
    state: &ShareApiState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
    let Some(pairing) = state.pairing.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN pairing is not enabled on this workshop".to_string(),
        ));
    };
    let Some(token) = bearer_token(&headers) else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Bearer session token required for remote share import".to_string(),
        ));
    };
    let authorized = pairing
        .authorize_bearer_token(token)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    if !authorized {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid or expired share session token".to_string(),
        ));
    }
    Ok(())
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn is_local_request(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}
