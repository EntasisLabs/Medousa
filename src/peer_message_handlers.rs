//! HTTP handlers for peer conversations (`/v1/peer/messages*`).

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::environment_store::environment_hub;
use crate::pairing::{PairedDeviceRecord, PairingService};
use crate::peer_messages::{
    append_message, build_message, get_message, list_messages, mark_read, messages_for_device,
    unread_count, PeerMessage, PeerMessageAttachmentSummary, PeerMessagePostRequest,
    PeerMessagesListResponse, PeerUnreadCountResponse,
};
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
}

pub fn peer_message_router(state: PeerMessageApiState) -> Router {
    Router::new()
        .route("/v1/peer/messages", get(list_peer_messages).post(post_peer_message))
        .route("/v1/peer/messages/unread-count", get(peer_unread_count))
        .route("/v1/peer/messages/{message_id}/read", post(read_peer_message))
        .with_state(state)
}

async fn list_peer_messages(
    State(state): State<PeerMessageApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<PeerMessagesListResponse>, (StatusCode, String)> {
    let messages = if is_local_request(addr.ip()) {
        list_messages(query.unread_only.unwrap_or(false))
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    } else {
        let record = authorize_remote_record(&state, &headers)?;
        let mut messages = messages_for_device(&record.phone_id)
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        if query.unread_only.unwrap_or(false) {
            // Peer's unread = outbound-from-host copies not yet read by them.
            messages.retain(|message| {
                message.direction == "out" && message.read_at.is_none()
            });
        }
        messages
    };
    Ok(Json(PeerMessagesListResponse { messages }))
}

async fn peer_unread_count(
    State(state): State<PeerMessageApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<PeerUnreadCountResponse>, (StatusCode, String)> {
    let unread = if is_local_request(addr.ip()) {
        unread_count().map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    } else {
        let record = authorize_remote_record(&state, &headers)?;
        messages_for_device(&record.phone_id)
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
            .into_iter()
            .filter(|message| message.direction == "out" && message.read_at.is_none())
            .count()
    };
    Ok(Json(PeerUnreadCountResponse { unread }))
}

async fn post_peer_message(
    State(state): State<PeerMessageApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PeerMessagePostRequest>,
) -> Result<Json<PeerMessage>, (StatusCode, String)> {
    let local = is_local_request(addr.ip());
    let remote_record = if local {
        None
    } else {
        Some(authorize_remote_record(&state, &headers)?)
    };

    let (default_direction, default_to_id, default_to_name, fallback_from_id, fallback_from_name) =
        if let Some(record) = &remote_record {
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

    // Remote peers cannot forge outbound copies on this host.
    let mut request = body;
    if remote_record.is_some() {
        request.direction = Some("in".to_string());
        request.from_device_id = Some(fallback_from_id.to_string());
        request.from_name = Some(fallback_from_name.to_string());
        request.to_device_id = Some(state.local_device_id.clone());
        request.to_name = Some(state.local_peer_name.clone());
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
    if message.direction == "in" {
        if let Some(bundle) = message.attachment.clone() {
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
        }
    } else {
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
    Ok(Json(stored))
}

async fn read_peer_message(
    State(state): State<PeerMessageApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Path(message_id): Path<String>,
) -> Result<Json<PeerMessage>, (StatusCode, String)> {
    if !is_local_request(addr.ip()) {
        let record = authorize_remote_record(&state, &headers)?;
        let message = get_message(&message_id)
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
            .ok_or_else(|| (StatusCode::NOT_FOUND, "message not found".to_string()))?;
        if !crate::peer_messages::involves_device(&message, &record.phone_id) {
            return Err((StatusCode::FORBIDDEN, "message not in your conversation".to_string()));
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
            "Bearer session token required for peer messages".to_string(),
        ));
    };
    let record = pairing
        .find_by_session_token(token)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "Invalid or expired peer session token".to_string(),
            )
        })?;
    if record.session_token_expiry < chrono::Utc::now() {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid or expired peer session token".to_string(),
        ));
    }
    if !record.role.allows_peer_surface() {
        return Err((
            StatusCode::FORBIDDEN,
            "This pairing cannot use peer messaging".to_string(),
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

fn is_local_request(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}
