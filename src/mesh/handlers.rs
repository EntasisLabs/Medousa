//! HTTP handlers for mesh registry / outbox / inbox / receipts (`/v1/mesh/*`).

use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::delegated_task::{
    DelegatedTaskControlAction, DelegatedTaskControlObservation, DelegatedTaskControlRequest,
    DelegatedTaskError, DelegatedTaskErrorKind, DelegatedTaskObservation, DelegatedTaskRequest,
    DelegatedTaskStatus, delegated_work_id, validate_task_control_request, validate_task_request,
};
use crate::mesh::delivery;
use crate::mesh::envelope::{
    DEFAULT_ENVELOPE_TTL_SECS, MeshCapability, MeshEnvelopedRequest, MeshInboundBody,
    payload_hash_hex, sign_envelope, verify_enveloped_payload,
};
use crate::mesh::intros::{self, MeshIntroCandidate, MeshIntroRecord, MeshIntroStatus};
use crate::mesh::outbox::{self, MeshOutboxItem, MeshOutboxStatus};
use crate::mesh::receipts::{self, MeshReceipt};
use crate::mesh::registry::{self, MeshPeerEndpoints, MeshPeerRecord};
use crate::mesh::{
    CAP_CLIENT_RENDEZVOUS, CAP_MESH_BUNDLE_PUSH, CAP_MESH_MESSAGE, CAP_TASK_REQUEST, inbox,
    record_has_capability,
};
use crate::pairing::{PairedDeviceRecord, PairingService};
use crate::peer_execution_policy::{
    AssistantWorkAdmission, PeerExecutionPolicyStore, TaskExecutionGrant, execution_tool_domain,
};
use crate::request_principal::{Capability, PrincipalKind, RequestPrincipal, TransportClass};

#[derive(Clone)]
pub struct MeshApiState {
    pub pairing: Option<Arc<PairingService>>,
    pub local_device_id: String,
    pub execution_policies: Arc<PeerExecutionPolicyStore>,
    pub delegated_task_executor: Option<Arc<dyn super::task::DelegatedTaskExecutor>>,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshIntrosResponse {
    intros: Vec<MeshIntroRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MeshIntroCandidatesResponse {
    candidates: Vec<MeshIntroCandidate>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MeshIntroListQuery {
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeshIntroRequestBody {
    target_device_id: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    endpoints: Option<MeshPeerEndpoints>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MeshIntroDecisionBody {
    #[serde(default)]
    endpoints: Option<MeshPeerEndpoints>,
}

pub fn mesh_router(state: MeshApiState) -> Router {
    mesh_surface().into_router().with_state(state)
}

pub fn mesh_surface() -> DeclaredRouter<MeshApiState> {
    DeclaredRouter::default()
        .route(
            peer_policy(axum::http::Method::GET, "/v1/mesh/peers", 1024),
            get(list_mesh_peers),
        )
        .route(
            peer_policy(
                axum::http::Method::PATCH,
                "/v1/mesh/peers/{device_id}",
                64 * 1024,
            ),
            patch(patch_mesh_peer),
        )
        .methods([
            (
                peer_policy(axum::http::Method::GET, "/v1/mesh/outbox", 1024),
                get(list_mesh_outbox),
            ),
            (
                peer_policy(axum::http::Method::POST, "/v1/mesh/outbox", 2 * 1024 * 1024),
                post(enqueue_mesh_outbox),
            ),
        ])
        .route(
            peer_policy(
                axum::http::Method::POST,
                "/v1/mesh/outbox/{item_id}/flush",
                1024,
            ),
            post(flush_mesh_outbox_item),
        )
        .route(
            peer_policy(axum::http::Method::GET, "/v1/mesh/inbox", 1024),
            get(list_mesh_inbox),
        )
        .route(
            peer_policy(axum::http::Method::POST, "/v1/mesh/tasks", 1024 * 1024),
            post(exchange_mesh_task),
        )
        .route(
            peer_policy(
                axum::http::Method::POST,
                "/v1/mesh/tasks/{work_id}/control",
                64 * 1024,
            ),
            post(control_mesh_task),
        )
        .methods([
            (
                peer_policy(axum::http::Method::GET, "/v1/mesh/receipts", 1024),
                get(list_mesh_receipts),
            ),
            (
                peer_policy(axum::http::Method::POST, "/v1/mesh/receipts", 64 * 1024),
                post(post_mesh_receipt),
            ),
        ])
        .route(
            peer_policy(axum::http::Method::GET, "/v1/mesh/intros/candidates", 1024),
            get(list_mesh_intro_candidates),
        )
        .methods([
            (
                peer_policy(axum::http::Method::GET, "/v1/mesh/intros", 1024),
                get(list_mesh_intros),
            ),
            (
                peer_policy(axum::http::Method::POST, "/v1/mesh/intros", 64 * 1024),
                post(request_mesh_intro),
            ),
        ])
        .route(
            peer_policy(
                axum::http::Method::POST,
                "/v1/mesh/intros/{intro_id}/accept",
                64 * 1024,
            ),
            post(accept_mesh_intro),
        )
        .route(
            peer_policy(
                axum::http::Method::POST,
                "/v1/mesh/intros/{intro_id}/decline",
                1024,
            ),
            post(decline_mesh_intro),
        )
}

fn peer_policy(method: axum::http::Method, path: &'static str, body_limit: usize) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::PeerExchange,
        required_capability: Some(Capability::PeerExchange),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit,
        rate_limit_class: RateLimitClass::PeerExchange,
    }
}

async fn list_mesh_peers(
    State(_state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Result<Json<MeshPeersResponse>, (StatusCode, String)> {
    require_local_or_portal(&principal)?;
    let peers = registry::list_peers().map_err(internal)?;
    Ok(Json(MeshPeersResponse { peers }))
}

async fn patch_mesh_peer(
    State(state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(device_id): Path<String>,
    Json(body): Json<MeshPeerPatchRequest>,
) -> Result<Json<MeshPeerRecord>, (StatusCode, String)> {
    require_local_or_portal(&principal)?;
    if registry::get_peer(&device_id).map_err(internal)?.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("mesh peer not found: {device_id}"),
        ));
    }
    let mut peer = registry::get_peer(&device_id)
        .map_err(internal)?
        .expect("peer checked");
    if let Some(enabled) = body.mesh_enabled {
        peer = registry::set_mesh_enabled(&device_id, enabled).map_err(internal)?;
    }
    if let Some(grants) = body.mesh_grants {
        if let Some(pairing) = state.pairing.as_ref() {
            let _ = pairing
                .set_mesh_grants(&device_id, grants.clone())
                .map_err(internal)?;
        }
        peer = registry::set_grants(&device_id, grants).map_err(internal)?;
    }
    if let Some(endpoints) = body.endpoints {
        peer = registry::set_endpoints(&device_id, endpoints).map_err(internal)?;
    }
    Ok(Json(peer))
}

async fn list_mesh_intro_candidates(
    State(state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Result<Json<MeshIntroCandidatesResponse>, (StatusCode, String)> {
    let caller = require_rendezvous_caller(&state, &principal)?;
    let pairing = state.pairing.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN pairing is not enabled on this workshop".to_string(),
        )
    })?;
    let candidates = intros::list_candidates(&caller.phone_id, |device_id| {
        pairing
            .find_by_phone_id(device_id)
            .ok()
            .flatten()
            .is_some_and(|record| record_has_capability(&record, CAP_CLIENT_RENDEZVOUS))
    })
    .map_err(internal)?;
    Ok(Json(MeshIntroCandidatesResponse { candidates }))
}

async fn list_mesh_intros(
    State(state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Query(query): Query<MeshIntroListQuery>,
) -> Result<Json<MeshIntrosResponse>, (StatusCode, String)> {
    let caller = require_rendezvous_caller(&state, &principal)?;
    let status = match query.status.as_deref().map(str::trim) {
        None | Some("") | Some("all") => None,
        Some(raw) => Some(MeshIntroStatus::parse(raw).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                format!("unknown intro status filter: {raw}"),
            )
        })?),
    };
    let intros = intros::list_for_caller(&caller.phone_id, status).map_err(internal)?;
    Ok(Json(MeshIntrosResponse { intros }))
}

async fn request_mesh_intro(
    State(state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(body): Json<MeshIntroRequestBody>,
) -> Result<Json<MeshIntroRecord>, (StatusCode, String)> {
    let caller = require_rendezvous_caller(&state, &principal)?;
    let pairing = state.pairing.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN pairing is not enabled on this workshop".to_string(),
        )
    })?;
    let target_id = body.target_device_id.trim();
    if target_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "targetDeviceId is required".to_string(),
        ));
    }
    let target = pairing
        .find_by_phone_id(target_id)
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("target device not paired: {target_id}"),
            )
        })?;
    if !record_has_capability(&target, CAP_CLIENT_RENDEZVOUS) {
        return Err((
            StatusCode::FORBIDDEN,
            "target does not have client.rendezvous".to_string(),
        ));
    }
    if let Some(endpoints) = body.endpoints.as_ref() {
        let _ = registry::set_endpoints(&caller.phone_id, endpoints.clone());
    }
    let intro = intros::request_intro(
        &caller.phone_id,
        &caller.phone_name,
        &target.phone_id,
        &target.phone_name,
        body.note,
        body.endpoints,
    )
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
    Ok(Json(intro))
}

async fn accept_mesh_intro(
    State(state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(intro_id): Path<String>,
    Json(body): Json<MeshIntroDecisionBody>,
) -> Result<Json<MeshIntroRecord>, (StatusCode, String)> {
    let caller = require_rendezvous_caller(&state, &principal)?;
    if let Some(endpoints) = body.endpoints.as_ref() {
        let _ = registry::set_endpoints(&caller.phone_id, endpoints.clone());
    }
    let intro =
        intros::accept_intro(&intro_id, &caller.phone_id, body.endpoints).map_err(|err| {
            let msg = err.to_string();
            if msg.contains("not found") {
                (StatusCode::NOT_FOUND, msg)
            } else {
                (StatusCode::BAD_REQUEST, msg)
            }
        })?;
    Ok(Json(intro))
}

async fn decline_mesh_intro(
    State(state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(intro_id): Path<String>,
) -> Result<Json<MeshIntroRecord>, (StatusCode, String)> {
    let caller = require_rendezvous_caller(&state, &principal)?;
    let intro = intros::decline_intro(&intro_id, &caller.phone_id).map_err(|err| {
        let msg = err.to_string();
        if msg.contains("not found") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::BAD_REQUEST, msg)
        }
    })?;
    Ok(Json(intro))
}

async fn list_mesh_outbox(
    State(_state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Query(query): Query<MeshOutboxListQuery>,
) -> Result<Json<MeshOutboxListResponse>, (StatusCode, String)> {
    require_trusted_local(&principal)?;
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
    Extension(principal): Extension<RequestPrincipal>,
    Json(body): Json<MeshOutboxEnqueueRequest>,
) -> Result<Json<MeshOutboxItem>, (StatusCode, String)> {
    require_trusted_local(&principal)?;
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
    Extension(principal): Extension<RequestPrincipal>,
    Path(item_id): Path<String>,
) -> Result<Json<MeshOutboxItem>, (StatusCode, String)> {
    require_trusted_local(&principal)?;
    let item = outbox::get_outbox_item(&item_id)
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("outbox item not found: {item_id}"),
            )
        })?;
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
        let failed =
            outbox::mark_failed(&item.id, "peer has no lanBaseUrl endpoint").map_err(internal)?;
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
        && let Ok(receipt) = serde_json::from_str::<MeshReceipt>(raw)
    {
        let _ = receipts::store_received(&receipt);
        let item = outbox::mark_acked(&item.id, &receipt).map_err(internal)?;
        return Ok(Json(item));
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
    Extension(principal): Extension<RequestPrincipal>,
    Query(query): Query<MeshLimitQuery>,
) -> Result<Json<MeshInboxListResponse>, (StatusCode, String)> {
    require_trusted_local(&principal)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let items = inbox::list_inbox(limit).map_err(internal)?;
    Ok(Json(MeshInboxListResponse { items }))
}

async fn exchange_mesh_task(
    State(state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(body): Json<MeshInboundBody<DelegatedTaskRequest>>,
) -> Result<Response, (StatusCode, String)> {
    require_pairing_principal(&principal)?;
    let record = authorize_remote_peer(&state, &principal)?;
    let (envelope, payload) = body.into_parts();
    let envelope = envelope.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "signed mesh envelope required for remote task delivery".to_string(),
        )
    })?;
    if envelope.sender_device_id.trim() != record.phone_id.trim()
        || envelope.recipient_device_id.trim() != state.local_device_id.trim()
    {
        return Err((
            StatusCode::UNAUTHORIZED,
            "delegated task envelope identities must exactly match the authenticated pairing"
                .to_string(),
        ));
    }
    validate_task_request(&payload).map_err(map_delegated_task_error)?;
    verify_enveloped_payload(
        &MeshEnvelopedRequest {
            envelope: envelope.clone(),
            payload: payload.clone(),
        },
        &record.phone_public_key,
        &record.phone_id,
        &state.local_device_id,
        MeshCapability::TaskRequest,
        true,
    )
    .map_err(|error| (StatusCode::UNAUTHORIZED, error.to_string()))?;

    let turn_id = payload
        .grant
        .turn_id
        .as_deref()
        .expect("validated delegated turn id");
    let work_id = delegated_work_id(&record.phone_id, turn_id);
    let task_execution_grant = resolve_task_execution_grant(
        &state,
        &record,
        &payload,
        &work_id,
        envelope.expires_at,
    )?;

    let payload_hash =
        payload_hash_hex(&payload).map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let pairing = state.pairing.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN pairing is not enabled on this workshop".to_string(),
        )
    })?;
    let accepted = delivery::accept_inbound_delivery(
        pairing.identity().signing_key(),
        &state.local_device_id,
        &envelope,
        &payload_hash,
    )
    .map_err(internal)?;
    delivery::bind_delivery_local_ref(&accepted.inbox_id, &work_id, &accepted.receipt.id)
        .map_err(internal)?;

    let executor = state.delegated_task_executor.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "delegated task execution is not configured".to_string(),
        )
    })?;
    let observation: DelegatedTaskObservation = executor
        .submit_or_observe(&record, &payload, &task_execution_grant)
        .await
        .map_err(map_delegated_task_error)?;
    let result_hash = payload_hash_hex(&observation)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let seq = registry::allocate_outbound_seq(&record.phone_id).map_err(internal)?;
    let result_envelope = sign_envelope(
        pairing.identity().signing_key(),
        &state.local_device_id,
        &record.phone_id,
        seq,
        MeshCapability::TaskResult,
        &result_hash,
        chrono::Duration::seconds(DEFAULT_ENVELOPE_TTL_SECS),
    );
    let receipt = delivery::receipt_header_value(&accepted.receipt).map_err(internal)?;
    Ok((
        [("x-medousa-mesh-receipt", receipt)],
        Json(MeshEnvelopedRequest {
            envelope: result_envelope,
            payload: observation,
        }),
    )
        .into_response())
}

async fn control_mesh_task(
    State(state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(work_id): Path<String>,
    Json(body): Json<MeshInboundBody<DelegatedTaskControlRequest>>,
) -> Result<Response, (StatusCode, String)> {
    require_pairing_principal(&principal)?;
    let sender = authorize_remote_peer(&state, &principal)?;
    let (envelope, request) = body.into_parts();
    let envelope = envelope.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            "signed mesh envelope required for remote worker control".to_string(),
        )
    })?;
    if envelope.sender_device_id.trim() != sender.phone_id.trim()
        || envelope.recipient_device_id.trim() != state.local_device_id.trim()
    {
        return Err((
            StatusCode::UNAUTHORIZED,
            "worker control envelope identities must exactly match the authenticated pairing"
                .to_string(),
        ));
    }
    validate_task_control_request(&request).map_err(map_delegated_task_error)?;
    if work_id.trim() != request.work_id {
        return Err((
            StatusCode::BAD_REQUEST,
            "worker control path does not match its signed payload".to_string(),
        ));
    }
    verify_enveloped_payload(
        &MeshEnvelopedRequest {
            envelope,
            payload: request.clone(),
        },
        &sender.phone_public_key,
        &sender.phone_id,
        &state.local_device_id,
        MeshCapability::TaskRequest,
        true,
    )
    .map_err(|error| (StatusCode::UNAUTHORIZED, error.to_string()))?;

    let store = crate::agent_runtime::turn_worker::turn_worker_store();
    let current = store.get(&request.work_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            "delegated worker was not found".to_string(),
        )
    })?;
    let expected_identity = format!("peer:{}", sender.phone_id.trim());
    if current.disposition
        != crate::agent_runtime::turn_worker::TurnWorkDisposition::Delegated
        || current.identity_user_id.as_deref() != Some(expected_identity.as_str())
    {
        return Err((
            StatusCode::FORBIDDEN,
            "delegated worker belongs to another execution authority".to_string(),
        ));
    }
    let grant = current.task_execution_grant.as_ref().ok_or_else(|| {
        (
            StatusCode::CONFLICT,
            "delegated worker has no task execution grant".to_string(),
        )
    })?;
    if grant.peer_device_id != sender.phone_id
        || grant.peer_pairing_id != sender.pairing_id
        || grant.work_id != request.work_id
        || grant.origin_runtime_id != request.parent_runtime_id
        || grant.parent_session_id != request.source_execution.session_id.as_str()
        || grant.correlation_id != request.correlation_id
        || current.parent_runtime_id != request.parent_runtime_id
        || current.parent_turn_correlation_id.as_deref() != Some(request.correlation_id.as_str())
    {
        return Err((
            StatusCode::CONFLICT,
            "delegated worker control provenance does not match admission".to_string(),
        ));
    }

    let updated = match request.action {
        DelegatedTaskControlAction::Cancel => store
            .cancel_delegated_exact(&request.work_id, &expected_identity)
            .map_err(map_delegated_control_error)?,
        DelegatedTaskControlAction::Steer => {
            if grant.expires_at <= chrono::Utc::now() {
                let _ = store.cancel_delegated_exact(&request.work_id, &expected_identity);
                return Err((
                    StatusCode::FORBIDDEN,
                    "delegated task execution grant expired".to_string(),
                ));
            }
            store
                .push_delegated_steer_exact(
                    &request.work_id,
                    &expected_identity,
                    &request.control_id,
                    request.message.clone().expect("validated steer message"),
                    Some(expected_identity.clone()),
                )
                .map_err(map_delegated_control_error)?
        }
    };
    let observation = DelegatedTaskControlObservation {
        schema_version: crate::delegated_task::DELEGATED_TASK_SCHEMA_VERSION,
        action: request.action,
        work_id: request.work_id,
        status: delegated_status(updated.status),
        queued_steers: updated.steer_messages.len(),
        destination_runtime_id: state.local_device_id.clone(),
    };
    let result_hash = payload_hash_hex(&observation)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let pairing = state.pairing.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN pairing is not enabled on this workshop".to_string(),
        )
    })?;
    let seq = registry::allocate_outbound_seq(&sender.phone_id).map_err(internal)?;
    let response_envelope = sign_envelope(
        pairing.identity().signing_key(),
        &state.local_device_id,
        &sender.phone_id,
        seq,
        MeshCapability::TaskResult,
        &result_hash,
        chrono::Duration::seconds(DEFAULT_ENVELOPE_TTL_SECS),
    );
    Ok(Json(MeshEnvelopedRequest {
        envelope: response_envelope,
        payload: observation,
    })
    .into_response())
}

fn delegated_status(
    status: crate::agent_runtime::turn_worker::TurnWorkStatus,
) -> DelegatedTaskStatus {
    match status {
        crate::agent_runtime::turn_worker::TurnWorkStatus::Pending => DelegatedTaskStatus::Pending,
        crate::agent_runtime::turn_worker::TurnWorkStatus::Running => DelegatedTaskStatus::Running,
        crate::agent_runtime::turn_worker::TurnWorkStatus::Completed => {
            DelegatedTaskStatus::Completed
        }
        crate::agent_runtime::turn_worker::TurnWorkStatus::Failed => DelegatedTaskStatus::Failed,
        crate::agent_runtime::turn_worker::TurnWorkStatus::Cancelled => {
            DelegatedTaskStatus::Cancelled
        }
    }
}

fn map_delegated_control_error(
    error: crate::agent_runtime::turn_worker::DelegatedWorkControlError,
) -> (StatusCode, String) {
    use crate::agent_runtime::turn_worker::DelegatedWorkControlError;
    match error {
        DelegatedWorkControlError::MissingWork => {
            (StatusCode::NOT_FOUND, "delegated worker was not found".to_string())
        }
        DelegatedWorkControlError::ForeignIdentity => (
            StatusCode::FORBIDDEN,
            "delegated worker belongs to another execution authority".to_string(),
        ),
        DelegatedWorkControlError::WrongDisposition => (
            StatusCode::CONFLICT,
            "work id does not identify a delegated worker".to_string(),
        ),
        DelegatedWorkControlError::NotActive => (
            StatusCode::CONFLICT,
            "delegated worker is no longer active".to_string(),
        ),
        DelegatedWorkControlError::SessionDeleting => (
            StatusCode::CONFLICT,
            "delegated worker session is being deleted".to_string(),
        ),
    }
}

fn resolve_task_execution_grant(
    state: &MeshApiState,
    sender: &PairedDeviceRecord,
    request: &DelegatedTaskRequest,
    work_id: &str,
    envelope_expires_at: chrono::DateTime<chrono::Utc>,
) -> Result<TaskExecutionGrant, (StatusCode, String)> {
    let store = crate::agent_runtime::turn_worker::turn_worker_store();
    if let Some(existing) = store.get(work_id) {
        let expected_identity = format!("peer:{}", sender.phone_id.trim());
        if existing.disposition
            != crate::agent_runtime::turn_worker::TurnWorkDisposition::Delegated
            || existing.identity_user_id.as_deref() != Some(expected_identity.as_str())
        {
            return Err((
                StatusCode::CONFLICT,
                "delegated work identity is already bound to another caller".to_string(),
            ));
        }
        if let Some(grant) = existing.task_execution_grant.clone() {
            if grant.peer_device_id != sender.phone_id
                || grant.peer_pairing_id != sender.pairing_id
                || grant.work_id != work_id
            {
                return Err((
                    StatusCode::CONFLICT,
                    "delegated work carries a conflicting execution grant".to_string(),
                ));
            }
            if grant.expires_at <= chrono::Utc::now()
                && matches!(
                    existing.status,
                    crate::agent_runtime::turn_worker::TurnWorkStatus::Pending
                        | crate::agent_runtime::turn_worker::TurnWorkStatus::Running
                )
            {
                let _ = store.cancel_exact(&existing.session_id, work_id);
            }
            // Idempotent observation stays bound to the original grant. A
            // later policy edit can cancel active work, but it cannot rewrite
            // the authority under which that work ran.
            return Ok(grant);
        }
    }

    let legacy_task_request_granted = record_has_capability(sender, CAP_TASK_REQUEST);
    if !legacy_task_request_granted {
        return Err((
            StatusCode::FORBIDDEN,
            "task.request grant required".to_string(),
        ));
    }
    let request_expires_at = request
        .grant
        .payload
        .get("deadline_at")
        .and_then(|value| serde_json::from_value::<chrono::DateTime<chrono::Utc>>(value.clone()).ok())
        .map(|deadline| deadline.min(envelope_expires_at))
        .unwrap_or(envelope_expires_at);
    let (worker_intent, bot_id, requested_tool_names) = request.worker.as_ref().map_or_else(
        || {
            (
                "research",
                None,
                crate::agent_runtime::turn_worker::REMOTE_DELEGATED_TOOL_CEILING
                    .iter()
                    .map(|name| (*name).to_string())
                    .collect::<Vec<_>>(),
            )
        },
        |worker| {
            (
                worker.intent.as_str(),
                worker.parent.bot.as_ref().map(|bot| bot.bot_id.as_str()),
                worker.tools.names.clone(),
            )
        },
    );
    let requested_tool_domain_values = requested_tool_names
        .iter()
        .map(|name| execution_tool_domain(name).to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let requested_tool_domains = requested_tool_domain_values
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let requested_tool_name_refs = requested_tool_names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    state
        .execution_policies
        .admit_assistant_work(AssistantWorkAdmission {
            peer_device_id: &sender.phone_id,
            peer_pairing_id: &sender.pairing_id,
            origin_runtime_id: &request.parent_runtime_id,
            destination_runtime_id: &state.local_device_id,
            parent_session_id: &request.grant.session_id,
            bot_id,
            work_id,
            correlation_id: &request.grant.correlation_id,
            worker_intent,
            requested_tool_domains: &requested_tool_domains,
            requested_tool_names: &requested_tool_name_refs,
            request_expires_at,
            legacy_task_request_granted,
        })
        .map_err(internal)?
        .map_err(|denial| (StatusCode::FORBIDDEN, denial.code().to_string()))
}

async fn list_mesh_receipts(
    State(_state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Query(query): Query<MeshLimitQuery>,
) -> Result<Json<MeshReceiptsListResponse>, (StatusCode, String)> {
    require_trusted_local(&principal)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let receipts = receipts::list_receipts(limit).map_err(internal)?;
    Ok(Json(MeshReceiptsListResponse { receipts }))
}

async fn post_mesh_receipt(
    State(_state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(receipt): Json<MeshReceipt>,
) -> Result<Json<MeshReceipt>, (StatusCode, String)> {
    if principal.transport() != TransportClass::Loopback {
        let _record = authorize_remote_peer(&_state, &principal)?;
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

fn require_trusted_local(principal: &RequestPrincipal) -> Result<(), (StatusCode, String)> {
    if principal.transport() == TransportClass::Loopback {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        "mesh admin routes require trusted local access".to_string(),
    ))
}

fn require_pairing_principal(principal: &RequestPrincipal) -> Result<(), (StatusCode, String)> {
    if matches!(
        principal.kind(),
        PrincipalKind::Peer | PrincipalKind::Portal | PrincipalKind::Root
    ) {
        return Ok(());
    }
    Err((
        StatusCode::UNAUTHORIZED,
        "daemon-to-daemon tasks require an authenticated pairing principal".to_string(),
    ))
}

fn require_local_or_portal(principal: &RequestPrincipal) -> Result<(), (StatusCode, String)> {
    if principal.transport() == TransportClass::Loopback
        || matches!(
            principal.kind(),
            PrincipalKind::Portal | PrincipalKind::Root
        )
    {
        return Ok(());
    }
    Err((
        StatusCode::FORBIDDEN,
        "peer credentials cannot list mesh registry".to_string(),
    ))
}

/// Bearer (or trusted local acting only via bearer) must have `client.rendezvous`.
fn require_rendezvous_caller(
    state: &MeshApiState,
    principal: &RequestPrincipal,
) -> Result<PairedDeviceRecord, (StatusCode, String)> {
    let record = authorize_remote_peer(state, principal)?;
    if !record_has_capability(&record, CAP_CLIENT_RENDEZVOUS) {
        return Err((
            StatusCode::FORBIDDEN,
            "client.rendezvous grant required".to_string(),
        ));
    }
    Ok(record)
}

fn authorize_remote_peer(
    state: &MeshApiState,
    principal: &RequestPrincipal,
) -> Result<PairedDeviceRecord, (StatusCode, String)> {
    let Some(pairing) = state.pairing.as_ref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN pairing is not enabled on this workshop".to_string(),
        ));
    };
    if !principal.capabilities().contains(Capability::PeerExchange) {
        return Err((
            StatusCode::FORBIDDEN,
            "This credential cannot use mesh surfaces".to_string(),
        ));
    }
    let Some(credential_id) = principal.credential_id() else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Authenticated pairing required".to_string(),
        ));
    };
    let record = pairing
        .find_by_pairing_id(credential_id.as_str())
        .map_err(internal)?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired credential".to_string(),
            )
        })?;
    Ok(record)
}

fn internal(err: impl ToString) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

fn map_delegated_task_error(error: DelegatedTaskError) -> (StatusCode, String) {
    let status = match error.kind {
        DelegatedTaskErrorKind::Invalid => StatusCode::BAD_REQUEST,
        DelegatedTaskErrorKind::Conflict => StatusCode::CONFLICT,
        DelegatedTaskErrorKind::Transport => StatusCode::GATEWAY_TIMEOUT,
        DelegatedTaskErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, error.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pairing::PairingRole;

    fn remote_principal(role: PairingRole) -> RequestPrincipal {
        let now = chrono::Utc::now();
        RequestPrincipal::from_pairing_record(
            PairedDeviceRecord {
                pairing_id: "pairing-1".into(),
                phone_id: "phone-1".into(),
                phone_name: "Phone".into(),
                phone_public_key: "key".into(),
                paired_at: now,
                last_seen: now,
                session_token_hash: "hash".into(),
                session_token_expiry: now,
                trust_expires_at: None,
                idle_timeout_seconds: None,
                credential_generation: 1,
                role,
                profile_id: None,
                mesh_grants: Vec::new(),
                apns_device_token: None,
                push_platform: None,
                push_updated_at: None,
                live_activity_push_token: None,
                live_activity_push_updated_at: None,
            },
            TransportClass::Direct,
            false,
        )
    }

    #[test]
    fn mesh_inventory_is_complete_and_peer_scoped() {
        let entries = mesh_surface().inventory().entries().collect::<Vec<_>>();
        assert_eq!(entries.len(), 14);
        assert!(entries.iter().all(|entry| {
            entry.group == RouteGroup::PeerExchange
                && entry.required_capability == Some("peer.exchange")
                && !entry.bootstrap_public
                && entry.body_limit > 0
        }));
        let outbox = entries
            .iter()
            .filter(|entry| entry.path == "/v1/mesh/outbox")
            .collect::<Vec<_>>();
        assert_eq!(outbox.len(), 2);
        assert_eq!(outbox[0].method, "GET");
        assert_eq!(outbox[0].body_limit, 1024);
        assert_eq!(outbox[1].method, "POST");
        assert_eq!(outbox[1].body_limit, 2 * 1024 * 1024);
    }

    #[test]
    fn local_only_checks_use_normalized_principal_transport() {
        assert!(
            require_trusted_local(&RequestPrincipal::local_app(
                Arc::from("test-local"),
                TransportClass::Loopback,
            ))
            .is_ok()
        );
        assert!(
            require_trusted_local(&RequestPrincipal::anonymous(TransportClass::Direct)).is_err()
        );
    }

    #[test]
    fn registry_access_accepts_portal_but_not_peer_principal() {
        assert!(require_local_or_portal(&remote_principal(PairingRole::Portal)).is_ok());
        assert!(require_local_or_portal(&remote_principal(PairingRole::Peer)).is_err());
    }
}
