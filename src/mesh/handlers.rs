//! HTTP handlers for mesh registry / outbox / inbox / receipts (`/v1/mesh/*`).

use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::mesh::envelope::MeshCapability;
use crate::mesh::outbox::{self, MeshOutboxItem, MeshOutboxStatus};
use crate::mesh::receipts::{self, MeshReceipt};
use crate::mesh::registry::{self, MeshPeerEndpoints, MeshPeerRecord};
use crate::mesh::{inbox, CAP_MESH_BUNDLE_PUSH, CAP_MESH_MESSAGE, CAP_TASK_REQUEST};
use crate::pairing::{PairedDeviceRecord, PairingRole, PairingService};

#[derive(Clone)]
pub struct MeshApiState {
    pub pairing: Option<Arc<PairingService>>,
    pub local_device_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshPeersResponse {
    peers: Vec<MeshPeerRecord>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MeshPeerPatchRequest {
    #[serde(default)]
    mesh_enabled: Option<bool>,
    #[serde(default)]
    mesh_grants: Option<Vec<String>>,
    #[serde(default)]
    endpoints: Option<MeshPeerEndpoints>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshOutboxListResponse {
    items: Vec<MeshOutboxItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeshOutboxListQuery {
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeshOutboxEnqueueRequest {
    peer_device_id: String,
    #[serde(default)]
    capability: Option<String>,
    payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshInboxListResponse {
    items: Vec<inbox::MeshInboxItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeshLimitQuery {
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshReceiptsListResponse {
    receipts: Vec<MeshReceipt>,
}

pub fn mesh_router(state: MeshApiState) -> Router {
    Router::new()
        .route("/v1/mesh/peers", get(list_mesh_peers))
        .route("/v1/mesh/peers/{device_id}", patch(patch_mesh_peer))
        .route("/v1/mesh/outbox", get(list_mesh_outbox).post(enqueue_mesh_outbox))
        .route("/v1/mesh/outbox/{item_id}/flush", post(flush_mesh_outbox_item))
        .route("/v1/mesh/inbox", get(list_mesh_inbox))
        .route("/v1/mesh/receipts", get(list_mesh_receipts).post(post_mesh_receipt))
        .with_state(state)
}

async fn list_mesh_peers(
    State(state): State<MeshApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<MeshPeersResponse>, (StatusCode, String)> {
    require_local_or_portal(&state, addr.ip(), &headers)?;
    let peers = registry::list_peers().map_err(internal)?;
    Ok(Json(MeshPeersResponse { peers }))
}

async fn patch_mesh_peer(
    State(state): State<MeshApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Path(device_id): Path<String>,
    Json(body): Json<MeshPeerPatchRequest>,
) -> Result<Json<MeshPeerRecord>, (StatusCode, String)> {
    require_local_or_portal(&state, addr.ip(), &headers)?;
    if registry::get_peer(&device_id)
        .map_err(internal)?
        .is_none()
    {
        return Err((StatusCode::NOT_FOUND, format!("mesh peer not found: {device_id}")));
    }
    let mut peer = registry::get_peer(&device_id)
        .map_err(internal)?
        .expect("peer checked");
    if let Some(enabled) = body.mesh_enabled {
        peer = registry::set_mesh_enabled(&device_id, enabled).map_err(internal)?;
    }
    if let Some(grants) = body.mesh_grants {
        peer = registry::set_grants(&device_id, grants).map_err(internal)?;
    }
    if let Some(endpoints) = body.endpoints {
        peer = registry::set_endpoints(&device_id, endpoints).map_err(internal)?;
    }
    Ok(Json(peer))
}

async fn list_mesh_outbox(
    State(_state): State<MeshApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<MeshOutboxListQuery>,
) -> Result<Json<MeshOutboxListResponse>, (StatusCode, String)> {
    require_trusted_local(addr.ip(), &headers)?;
    let status = match query.status.as_deref().map(str::trim) {
        None | Some("") => None,
        Some("pending") => Some(MeshOutboxStatus::Pending),
        Some("inFlight") | Some("in_flight") => Some(MeshOutboxStatus::InFlight),
        Some("acked") => Some(MeshOutboxStatus::Acked),
        Some("failed") => Some(MeshOutboxStatus::Failed),
        Some(other) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown outbox status filter: {other}"),
            ));
        }
    };
    let items = outbox::list_outbox(status).map_err(internal)?;
    Ok(Json(MeshOutboxListResponse { items }))
}

async fn enqueue_mesh_outbox(
    State(state): State<MeshApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<MeshOutboxEnqueueRequest>,
) -> Result<Json<MeshOutboxItem>, (StatusCode, String)> {
    require_trusted_local(addr.ip(), &headers)?;
    let pairing = state.pairing.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN pairing is not enabled on this workshop".to_string(),
        )
    })?;
    let capability = match body
        .capability
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None | Some(CAP_MESH_MESSAGE) => MeshCapability::Message,
        Some(CAP_MESH_BUNDLE_PUSH) => MeshCapability::BundlePush,
        Some(CAP_TASK_REQUEST) => MeshCapability::TaskRequest,
        Some(other) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("unknown mesh capability: {other}"),
            ));
        }
    };
    let item = outbox::enqueue(
        pairing.identity().signing_key(),
        &state.local_device_id,
        body.peer_device_id.trim(),
        capability,
        body.payload,
    )
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(item))
}

async fn flush_mesh_outbox_item(
    State(state): State<MeshApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Path(item_id): Path<String>,
) -> Result<Json<MeshOutboxItem>, (StatusCode, String)> {
    require_trusted_local(addr.ip(), &headers)?;
    let item = outbox::get_outbox_item(&item_id)
        .map_err(internal)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("outbox item not found: {item_id}")))?;
    if item.status == MeshOutboxStatus::Acked {
        return Ok(Json(item));
    }
    let peer = registry::get_peer(&item.peer_device_id)
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                format!("mesh peer not registered: {}", item.peer_device_id),
            )
        })?;
    let Some(base) = peer
        .endpoints
        .lan_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        let failed = outbox::mark_failed(&item.id, "peer has no lanBaseUrl endpoint")
            .map_err(internal)?;
        return Err((
            StatusCode::CONFLICT,
            failed
                .last_error
                .unwrap_or_else(|| "peer has no lanBaseUrl endpoint".to_string()),
        ));
    };

    let _ = outbox::mark_in_flight(&item.id).map_err(internal)?;
    let wrapped = outbox::enveloped_request(&item);
    let client = reqwest::Client::new();
    let url = format!("{}/v1/peer/messages", base.trim_end_matches('/'));
    let response = client
        .post(&url)
        .json(&wrapped)
        .send()
        .await
        .map_err(|err| {
            let _ = outbox::mark_failed(&item.id, &err.to_string());
            (StatusCode::BAD_GATEWAY, err.to_string())
        })?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let msg = format!("peer deliver HTTP {status}: {body}");
        let _ = outbox::mark_failed(&item.id, &msg);
        return Err((StatusCode::BAD_GATEWAY, msg));
    }

    if let Some(raw) = response
        .headers()
        .get("x-medousa-mesh-receipt")
        .and_then(|value| value.to_str().ok())
    {
        if let Ok(receipt) = serde_json::from_str::<MeshReceipt>(raw) {
            let _ = receipts::store_received(&receipt);
            let item = outbox::mark_acked(&item.id, &receipt).map_err(internal)?;
            return Ok(Json(item));
        }
    }

    // Soft-ack when the peer accepted the POST but did not return a receipt header.
    let soft = MeshReceipt {
        id: format!("mrc_soft_{}", item.id),
        version: receipts::MESH_RECEIPT_VERSION,
        sender_device_id: item.peer_device_id.clone(),
        recipient_device_id: state.local_device_id.clone(),
        ack_seq: item.seq,
        payload_hash: item.envelope.payload_hash.clone(),
        status: receipts::MeshReceiptStatus::Delivered,
        issued_at: chrono::Utc::now(),
        signature: "soft".to_string(),
    };
    let item = outbox::mark_acked(&item.id, &soft).map_err(internal)?;
    Ok(Json(item))
}

async fn list_mesh_inbox(
    State(_state): State<MeshApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<MeshLimitQuery>,
) -> Result<Json<MeshInboxListResponse>, (StatusCode, String)> {
    require_trusted_local(addr.ip(), &headers)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let items = inbox::list_inbox(limit).map_err(internal)?;
    Ok(Json(MeshInboxListResponse { items }))
}

async fn list_mesh_receipts(
    State(_state): State<MeshApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<MeshLimitQuery>,
) -> Result<Json<MeshReceiptsListResponse>, (StatusCode, String)> {
    require_trusted_local(addr.ip(), &headers)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let receipts = receipts::list_receipts(limit).map_err(internal)?;
    Ok(Json(MeshReceiptsListResponse { receipts }))
}

async fn post_mesh_receipt(
    State(_state): State<MeshApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(receipt): Json<MeshReceipt>,
) -> Result<Json<MeshReceipt>, (StatusCode, String)> {
    if !crate::remote_trust::is_trusted_local(addr.ip(), &headers) {
        let _record = authorize_remote_peer(&_state, &headers)?;
    }
    receipts::store_received(&receipt).map_err(internal)?;
    // Receipt.sender = remote host that received our delivery; that host is our outbox peer.
    if let Some(item) =
        outbox::find_by_peer_seq(&receipt.sender_device_id, receipt.ack_seq).map_err(internal)?
    {
        let _ = outbox::mark_acked(&item.id, &receipt);
    }
    Ok(Json(receipt))
}

fn require_trusted_local(
    ip: std::net::IpAddr,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
    if crate::remote_trust::is_trusted_local(ip, headers) {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        "mesh admin routes require trusted local access".to_string(),
    ))
}

fn require_local_or_portal(
    state: &MeshApiState,
    ip: std::net::IpAddr,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
    if crate::remote_trust::is_trusted_local(ip, headers) {
        return Ok(());
    }
    let record = authorize_remote_peer(state, headers)?;
    if record.role.allows_full_portal() {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        "peer credentials cannot list mesh registry".to_string(),
    ))
}

fn authorize_remote_peer(
    state: &MeshApiState,
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
            "Bearer session token required".to_string(),
        ));
    };
    let record = pairing
        .find_by_session_token(token)
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired session token".to_string(),
            )
        })?;
    if record.session_token_expiry < chrono::Utc::now() {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid or expired session token".to_string(),
        ));
    }
    if !record.role.allows_peer_surface() && record.role != PairingRole::Portal {
        return Err((
            StatusCode::FORBIDDEN,
            "This pairing cannot use mesh surfaces".to_string(),
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

fn internal(err: impl ToString) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
