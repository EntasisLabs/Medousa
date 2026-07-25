//! HTTP handlers for workshop sharing (`/v1/share/*`).

use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::environment_store::environment_hub;
use crate::mesh::{
    MeshCapability, MeshInboundBody, record_has_capability, require_remote_envelope,
    CAP_MESH_BUNDLE_PUSH,
};
use crate::pairing::{PairedDeviceRecord, PairingService};
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
    Json(body): Json<MeshInboundBody<serde_json::Value>>,
) -> Result<Json<ShareImportResult>, (StatusCode, String)> {
    let body = authorize_and_unwrap_share(&state, addr.ip(), &headers, body)?;
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
    Json(body): Json<MeshInboundBody<serde_json::Value>>,
) -> Result<Json<ShareImportResult>, (StatusCode, String)> {
    share_import(State(state), ConnectInfo(addr), headers, Json(body)).await
}

fn authorize_and_unwrap_share(
    state: &ShareApiState,
    ip: std::net::IpAddr,
    headers: &HeaderMap,
    body: MeshInboundBody<serde_json::Value>,
) -> Result<ShareImportRequest, (StatusCode, String)> {
    if crate::remote_trust::is_trusted_local(ip, headers) {
        let (_envelope, payload) = body.into_parts();
        return serde_json::from_value(payload)
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()));
    }
    let record = authorize_remote_share_record(state, headers)?;
    require_remote_envelope(
        body,
        true,
        &record.phone_public_key,
        &record.phone_id,
        &state.local_device_id,
        MeshCapability::BundlePush,
        record_has_capability(&record, CAP_MESH_BUNDLE_PUSH),
    )
    .map_err(mesh_status)
}

fn authorize_remote_share_record(
    state: &ShareApiState,
    headers: &HeaderMap,
) -> Result<PairedDeviceRecord, (StatusCode, String)> {
    let Some(pairing) = state.pairing.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN pairing is not enabled on this workshop".to_string(),
        ));
    };
    let Some(token) = bearer_token(headers) else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Bearer session token required for remote share import".to_string(),
        ));
    };
    let record = pairing
        .find_by_session_token(token)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired share session token".to_string(),
            )
        })?;
    if record.session_token_expiry < chrono::Utc::now() {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid or expired share session token".to_string(),
        ));
    }
    if !record.role.allows_peer_surface() {
        return Err((
            StatusCode::FORBIDDEN,
            "This pairing cannot use share import".to_string(),
        ));
    }
    Ok(record)
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn mesh_status(err: crate::mesh::MeshEnvelopeError) -> (StatusCode, String) {
    use crate::mesh::MeshEnvelopeError::*;
    let status = match &err {
        MissingEnvelope | BadSignature(_) | BadPublicKey(_) | Expired | NotYetValid
        | PayloadHashMismatch | SenderMismatch | UnsupportedVersion(_) => StatusCode::UNAUTHORIZED,
        CapabilityNotGranted(_) | UnknownCapability | RecipientMismatch => StatusCode::FORBIDDEN,
        Serialize(_) => StatusCode::BAD_REQUEST,
    };
    (status, err.to_string())
}
