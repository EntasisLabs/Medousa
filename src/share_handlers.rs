//! HTTP handlers for workshop sharing (`/v1/share/*`).

use std::sync::Arc;

use axum::extract::{Extension, State};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::environment_store::environment_hub;
use crate::mesh::delivery::{
    accept_inbound_delivery, bind_delivery_local_ref, receipt_header_value,
};
use crate::mesh::{
    MeshCapability, MeshEnvelope, MeshInboundBody, MeshReceipt, record_has_capability,
    require_remote_envelope, CAP_MESH_BUNDLE_PUSH,
};
use crate::pairing::{PairedDeviceRecord, PairingService};
use crate::request_principal::{Capability, RequestPrincipal, TransportClass};
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
    share_surface().into_router().with_state(state)
}

pub fn share_surface() -> DeclaredRouter<ShareApiState> {
    DeclaredRouter::default()
        .route(
            peer_policy(axum::http::Method::GET, "/v1/share/capabilities", 1024),
            get(share_capabilities),
        )
        .route(
            peer_policy(axum::http::Method::POST, "/v1/share/export", 1024 * 1024),
            post(share_export),
        )
        .route(
            peer_policy(
                axum::http::Method::POST,
                "/v1/share/import",
                8 * 1024 * 1024,
            ),
            post(share_import),
        )
        .route(
            peer_policy(axum::http::Method::POST, "/v1/share/push", 8 * 1024 * 1024),
            post(share_push),
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
    Extension(principal): Extension<RequestPrincipal>,
    Json(body): Json<MeshInboundBody<serde_json::Value>>,
) -> Result<Json<ShareImportResult>, (StatusCode, String)> {
    let (request, _envelope, _receipt) = authorize_and_unwrap_share(&state, &principal, body)?;
    let errors = request.bundle.validate();
    if !errors.is_empty() {
        return Err((StatusCode::BAD_REQUEST, errors.join("; ")));
    }
    import_bundle(environment_hub(), request)
        .await
        .map(Json)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))
}

async fn share_push(
    State(state): State<ShareApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(body): Json<MeshInboundBody<serde_json::Value>>,
) -> Result<Response, (StatusCode, String)> {
    let (request, envelope, mut receipt) = authorize_and_unwrap_share(&state, &principal, body)?;
    let errors = request.bundle.validate();
    if !errors.is_empty() {
        return Err((StatusCode::BAD_REQUEST, errors.join("; ")));
    }

    // Duplicate mesh delivery — ack without re-importing.
    if let Some(existing) = receipt.as_ref()
        && matches!(
            existing.status,
            crate::mesh::MeshReceiptStatus::Duplicate
        )
    {
        return Ok(with_mesh_receipt(
            ShareImportResult::default(),
            receipt.take(),
        ));
    }

    let result = import_bundle(environment_hub(), request)
        .await
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    if let (Some(env), Some(rcpt)) = (envelope.as_ref(), receipt.as_ref())
        && let Ok(Some(item)) =
            crate::mesh::inbox::find_by_sender_seq(&env.sender_device_id, env.seq)
    {
        let local_ref = format!("share_{}_{}", env.sender_device_id, env.seq);
        let _ = bind_delivery_local_ref(&item.id, &local_ref, &rcpt.id);
    }

    Ok(with_mesh_receipt(result, receipt))
}

type ShareUnwrapResult = Result<
    (
        ShareImportRequest,
        Option<MeshEnvelope>,
        Option<MeshReceipt>,
    ),
    (StatusCode, String),
>;

fn authorize_and_unwrap_share(
    state: &ShareApiState,
    principal: &RequestPrincipal,
    body: MeshInboundBody<serde_json::Value>,
) -> ShareUnwrapResult {
    if principal.transport() == TransportClass::Loopback {
        let (_envelope, payload) = body.into_parts();
        let request = serde_json::from_value(payload)
            .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;
        return Ok((request, None, None));
    }
    let record = authorize_remote_share_record(state, principal)?;
    let (payload, envelope) = require_remote_envelope(
        body,
        true,
        &record.phone_public_key,
        &record.phone_id,
        &state.local_device_id,
        MeshCapability::BundlePush,
        record_has_capability(&record, CAP_MESH_BUNDLE_PUSH),
    )
    .map_err(mesh_status)?;

    let mut receipt = None;
    if let Some(ref env) = envelope {
        let pairing = state.pairing.as_ref().ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "LAN pairing is not enabled on this workshop".to_string(),
            )
        })?;
        let accepted = accept_inbound_delivery(
            pairing.identity().signing_key(),
            &state.local_device_id,
            env,
            &env.payload_hash,
        )
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        receipt = Some(accepted.receipt);
        if accepted.duplicate {
            return Ok((payload, envelope, receipt));
        }
    }
    Ok((payload, envelope, receipt))
}

fn authorize_remote_share_record(
    state: &ShareApiState,
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
            "This credential cannot use share import".to_string(),
        ));
    }
    let Some(credential_id) = principal.credential_id() else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Authenticated pairing required for remote share import".to_string(),
        ));
    };
    let record = pairing
        .find_by_pairing_id(credential_id.as_str())
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired share credential".to_string(),
            )
        })?;
    Ok(record)
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

fn with_mesh_receipt(result: ShareImportResult, receipt: Option<MeshReceipt>) -> Response {
    let mut response = Json(result).into_response();
    if let Some(receipt) = receipt
        && let Ok(value) = receipt_header_value(&receipt)
        && let Ok(header) = HeaderValue::from_str(&value)
    {
        response
            .headers_mut()
            .insert("x-medousa-mesh-receipt", header);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_inventory_is_peer_scoped_and_bounded() {
        let entries = share_surface().inventory().entries().collect::<Vec<_>>();
        assert_eq!(entries.len(), 4);
        assert!(entries.iter().all(|entry| {
            entry.group == RouteGroup::PeerExchange
                && entry.required_capability == Some("peer.exchange")
                && !entry.bootstrap_public
                && entry.body_limit > 0
        }));
        assert_eq!(entries[2].path, "/v1/share/import");
        assert_eq!(entries[2].body_limit, 8 * 1024 * 1024);
    }
}
