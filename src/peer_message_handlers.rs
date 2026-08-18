//! HTTP handlers for peer conversations (`/v1/peer/messages*`).

use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
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
    CAP_MESH_MESSAGE, MeshCapability, MeshEnvelope, MeshInboundBody, MeshReceipt,
    record_has_capability, require_remote_envelope,
};
use crate::pairing::{PairedDeviceRecord, PairingRole, PairingService};
use crate::peer_messages::{
    PeerMessage, PeerMessageAttachmentSummary, PeerMessagePostRequest, PeerMessagesListResponse,
    PeerUnreadCountResponse, append_message, build_message, get_message, involves_device,
    list_messages_filtered, list_messages_for_peer_device, mark_read, unread_count,
    unread_count_for_device,
};
use crate::request_principal::{Capability, RequestPrincipal, TransportClass};
use crate::share::bundle::{ShareConflictStrategy, ShareImportRequest};
use crate::share::service::import_bundle;

#[derive(Clone)]
pub struct PeerMessageApiState {
    pub pairing: Option<Arc<PairingService>>,
    pub local_device_id: String,
    pub local_peer_name: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    unread_only: Option<bool>,
    device_id: Option<String>,
}

pub fn peer_message_router(state: PeerMessageApiState) -> Router {
    peer_message_surface().into_router().with_state(state)
}

pub fn peer_message_surface() -> DeclaredRouter<PeerMessageApiState> {
    DeclaredRouter::default()
        .methods([
            (
                peer_policy(axum::http::Method::GET, "/v1/peer/messages", 1024),
                get(list_peer_messages),
            ),
            (
                peer_policy(
                    axum::http::Method::POST,
                    "/v1/peer/messages",
                    2 * 1024 * 1024,
                ),
                post(post_peer_message),
            ),
        ])
        .route(
            peer_policy(
                axum::http::Method::GET,
                "/v1/peer/messages/unread-count",
                1024,
            ),
            get(peer_unread_count),
        )
        .route(
            peer_policy(
                axum::http::Method::POST,
                "/v1/peer/messages/{message_id}/read",
                1024,
            ),
            post(read_peer_message),
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

async fn list_peer_messages(
    State(state): State<PeerMessageApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Query(query): Query<ListQuery>,
) -> Result<Json<PeerMessagesListResponse>, (StatusCode, String)> {
    let unread_only = query.unread_only.unwrap_or(false);
    let device_filter = query
        .device_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let messages = if principal.transport() == TransportClass::Loopback {
        list_messages_filtered(unread_only, device_filter)
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    } else {
        let record = authorize_remote_record(&state, &principal)?;
        if record.role.allows_full_portal() {
            list_messages_filtered(unread_only, device_filter)
                .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        } else {
            if device_filter.is_some() {
                return Err((
                    StatusCode::FORBIDDEN,
                    "Peer credentials cannot filter other conversations".to_string(),
                ));
            }
            list_messages_for_peer_device(&record.phone_id, unread_only)
                .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        }
    };
    Ok(Json(PeerMessagesListResponse { messages }))
}

async fn peer_unread_count(
    State(state): State<PeerMessageApiState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Result<Json<PeerUnreadCountResponse>, (StatusCode, String)> {
    let unread = if principal.transport() == TransportClass::Loopback {
        unread_count().map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    } else {
        let record = authorize_remote_record(&state, &principal)?;
        if record.role.allows_full_portal() {
            unread_count().map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        } else {
            unread_count_for_device(&record.phone_id)
                .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        }
    };
    Ok(Json(PeerUnreadCountResponse { unread }))
}

async fn post_peer_message(
    State(state): State<PeerMessageApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(body): Json<MeshInboundBody<serde_json::Value>>,
) -> Result<Response, (StatusCode, String)> {
    let local = principal.transport() == TransportClass::Loopback;
    let remote_record = if local {
        None
    } else {
        Some(authorize_remote_record(&state, &principal)?)
    };

    let mut mesh_receipt: Option<MeshReceipt> = None;
    let mut mesh_inbox_id: Option<String> = None;

    let body: PeerMessagePostRequest = if let Some(record) = remote_record.as_ref() {
        let (payload, envelope): (PeerMessagePostRequest, Option<MeshEnvelope>) =
            require_remote_envelope(
                body,
                true,
                &record.phone_public_key,
                &record.phone_id,
                &state.local_device_id,
                MeshCapability::Message,
                record_has_capability(record, CAP_MESH_MESSAGE),
            )
            .map_err(mesh_status)?;
        if let Some(envelope) = envelope {
            let pairing = state.pairing.as_ref().ok_or_else(|| {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "LAN pairing is not enabled on this workshop".to_string(),
                )
            })?;
            let accepted = accept_inbound_delivery(
                pairing.identity().signing_key(),
                &state.local_device_id,
                &envelope,
                &envelope.payload_hash,
            )
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
            mesh_receipt = Some(accepted.receipt.clone());
            mesh_inbox_id = Some(accepted.inbox_id.clone());
            if accepted.duplicate {
                if let Some(local_ref) = accepted.local_ref.as_deref().filter(|v| !v.is_empty())
                    && let Some(existing) = get_message(local_ref)
                        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
                {
                    return Ok(with_mesh_receipt(existing, mesh_receipt));
                }
                return Ok(with_mesh_receipt(
                    PeerMessage {
                        id: format!("dup_{}_{}", envelope.sender_device_id, envelope.seq),
                        from_device_id: envelope.sender_device_id.clone(),
                        from_name: record.phone_name.clone(),
                        body: payload.body.clone(),
                        sent_at: chrono::Utc::now(),
                        read_at: None,
                        direction: "in".to_string(),
                        to_device_id: Some(state.local_device_id.clone()),
                        to_name: Some(state.local_peer_name.clone()),
                        attachment: None,
                        attachment_result: None,
                        kind: payload.kind.clone(),
                    },
                    mesh_receipt,
                ));
            }
        }
        payload
    } else {
        // Trusted local host UI may post bare payloads.
        let (_envelope, payload) = body.into_parts();
        serde_json::from_value(payload).map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?
    };

    let portal_host_reply = remote_record.as_ref().is_some_and(|record| {
        record.role.allows_full_portal()
            && body
                .direction
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("out"))
            && body
                .to_device_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty())
    });

    if portal_host_reply {
        let to_device_id = body
            .to_device_id
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        if !is_paired_peer_device(&state, to_device_id)? {
            return Err((
                StatusCode::BAD_REQUEST,
                "toDeviceId must refer to a paired peer".to_string(),
            ));
        }
    }

    let (default_direction, default_to_id, default_to_name, fallback_from_id, fallback_from_name) =
        if portal_host_reply {
            (
                "out",
                None,
                None,
                state.local_device_id.as_str(),
                state.local_peer_name.as_str(),
            )
        } else if let Some(record) = &remote_record {
            (
                "in",
                Some(state.local_device_id.as_str()),
                Some(state.local_peer_name.as_str()),
                record.phone_id.as_str(),
                record.phone_name.as_str(),
            )
        } else if body
            .to_device_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty())
            || body.direction.as_deref() == Some("out")
        {
            (
                "out",
                None,
                None,
                state.local_device_id.as_str(),
                state.local_peer_name.as_str(),
            )
        } else {
            (
                "in",
                Some(state.local_device_id.as_str()),
                Some(state.local_peer_name.as_str()),
                "unknown",
                "Peer",
            )
        };

    // Remote peers cannot forge outbound copies; portal sudo may reply as the workshop.
    let mut request = body;
    if remote_record.is_some() && !portal_host_reply {
        request.direction = Some("in".to_string());
        request.from_device_id = Some(fallback_from_id.to_string());
        request.from_name = Some(fallback_from_name.to_string());
        request.to_device_id = Some(state.local_device_id.clone());
        request.to_name = Some(state.local_peer_name.clone());
    } else if portal_host_reply {
        request.direction = Some("out".to_string());
        request.from_device_id = Some(state.local_device_id.clone());
        request.from_name = Some(state.local_peer_name.clone());
    }

    let mut message = build_message(
        request,
        fallback_from_id,
        fallback_from_name,
        default_direction,
        default_to_id,
        default_to_name,
    )
    .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    // Auto-import attachments only for inbound deliveries.
    if message.direction == "in"
        && let Some(bundle) = message.attachment.clone()
    {
        match import_bundle(
            environment_hub(),
            ShareImportRequest {
                bundle,
                conflict_strategy: ShareConflictStrategy::Rename,
                profile_id: None,
            },
        )
        .await
        {
            Ok(result) => {
                let summary = format!(
                    "Imported {} artifact(s), {} note(s)",
                    result.artifacts_imported, result.vault_notes_imported
                );
                message.attachment_result = Some(PeerMessageAttachmentSummary {
                    imported: true,
                    summary: Some(summary),
                    artifacts_imported: Some(result.artifacts_imported),
                    vault_notes_imported: Some(result.vault_notes_imported),
                });
                message.attachment = None;
            }
            Err(err) => {
                message.attachment_result = Some(PeerMessageAttachmentSummary {
                    imported: false,
                    summary: Some(err.to_string()),
                    artifacts_imported: None,
                    vault_notes_imported: None,
                });
            }
        }
    } else if message.direction != "in" {
        // Outbound copies keep a light attachment summary only.
        if message.attachment.is_some() {
            message.attachment_result = Some(PeerMessageAttachmentSummary {
                imported: false,
                summary: Some("Attachment sent".to_string()),
                artifacts_imported: None,
                vault_notes_imported: None,
            });
            message.attachment = None;
        }
    }

    let stored = append_message(message)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    if let (Some(inbox_id), Some(receipt)) = (mesh_inbox_id.as_deref(), mesh_receipt.as_ref()) {
        let _ = bind_delivery_local_ref(inbox_id, &stored.id, &receipt.id);
    }
    Ok(with_mesh_receipt(stored, mesh_receipt))
}

fn with_mesh_receipt(message: PeerMessage, receipt: Option<MeshReceipt>) -> Response {
    let mut response = Json(message).into_response();
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

async fn read_peer_message(
    State(state): State<PeerMessageApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(message_id): Path<String>,
) -> Result<Json<PeerMessage>, (StatusCode, String)> {
    if principal.transport() != TransportClass::Loopback {
        let record = authorize_remote_record(&state, &principal)?;
        if !record.role.allows_full_portal() {
            let message = get_message(&message_id)
                .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
                .ok_or_else(|| (StatusCode::NOT_FOUND, "message not found".to_string()))?;
            if !involves_device(&message, &record.phone_id) {
                return Err((
                    StatusCode::FORBIDDEN,
                    "message not in your conversation".to_string(),
                ));
            }
        }
    }
    mark_read(&message_id).map(Json).map_err(|err| {
        if err.to_string().contains("not found") {
            (StatusCode::NOT_FOUND, err.to_string())
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        }
    })
}

fn authorize_remote_record(
    state: &PeerMessageApiState,
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
            "This credential cannot use peer messaging".to_string(),
        ));
    }
    let Some(credential_id) = principal.credential_id() else {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Authenticated pairing required for peer messages".to_string(),
        ));
    };
    let record = pairing
        .find_by_pairing_id(credential_id.as_str())
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired peer credential".to_string(),
            )
        })?;
    Ok(record)
}

fn is_paired_peer_device(
    state: &PeerMessageApiState,
    device_id: &str,
) -> Result<bool, (StatusCode, String)> {
    let Some(pairing) = state.pairing.as_ref() else {
        return Ok(false);
    };
    let paired = pairing
        .list_paired_devices()
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(paired.into_iter().any(|record| {
        record.role == PairingRole::Peer && involves_device_id(&record.phone_id, device_id)
    }))
}

fn involves_device_id(left: &str, right: &str) -> bool {
    if left.is_empty() || right.is_empty() {
        return left == right;
    }
    left == right
        || left.starts_with(&right[..right.len().min(8)])
        || right.starts_with(&left[..left.len().min(8)])
}

fn mesh_status(err: crate::mesh::MeshEnvelopeError) -> (StatusCode, String) {
    use crate::mesh::MeshEnvelopeError::*;
    let status = match &err {
        MissingEnvelope
        | BadSignature(_)
        | BadPublicKey(_)
        | Expired
        | NotYetValid
        | PayloadHashMismatch
        | SenderMismatch
        | UnsupportedVersion(_) => StatusCode::UNAUTHORIZED,
        CapabilityNotGranted(_) | UnknownCapability | RecipientMismatch => StatusCode::FORBIDDEN,
        Serialize(_) => StatusCode::BAD_REQUEST,
    };
    (status, err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_message_inventory_has_method_specific_limits() {
        let entries = peer_message_surface()
            .inventory()
            .entries()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].method, "GET");
        assert_eq!(entries[0].body_limit, 1024);
        assert_eq!(entries[1].method, "POST");
        assert_eq!(entries[1].body_limit, 2 * 1024 * 1024);
        assert!(entries.iter().all(|entry| {
            entry.group == RouteGroup::PeerExchange
                && entry.required_capability == Some("peer.exchange")
                && !entry.bootstrap_public
        }));
    }

    #[test]
    fn involves_device_id_prefix_match() {
        assert!(involves_device_id("abcdef123456", "abcdef12"));
        assert!(involves_device_id("abcdef12", "abcdef123456"));
        assert!(!involves_device_id("aaa", "bbb"));
    }
}
