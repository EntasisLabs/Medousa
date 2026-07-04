//! HTTP handlers for peer inbox (`/v1/peer/messages*`).

use std::net::IpAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header::AUTHORIZATION};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::environment_store::environment_hub;
use crate::pairing::PairingService;
use crate::peer_messages::{
    append_message, build_inbound_message, list_messages, mark_read, unread_count,
    PeerMessage, PeerMessageAttachmentSummary, PeerMessagePostRequest, PeerMessagesListResponse,
    PeerUnreadCountResponse,
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
    State(_state): State<PeerMessageApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Query(query): Query<ListQuery>,
) -> Result<Json<PeerMessagesListResponse>, (StatusCode, String)> {
    require_local(addr.ip())?;
    let messages = list_messages(query.unread_only.unwrap_or(false))
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(PeerMessagesListResponse { messages }))
}

async fn peer_unread_count(
    State(_state): State<PeerMessageApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
) -> Result<Json<PeerUnreadCountResponse>, (StatusCode, String)> {
    require_local(addr.ip())?;
    let unread = unread_count().map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(PeerUnreadCountResponse { unread }))
}

async fn post_peer_message(
    State(state): State<PeerMessageApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(body): Json<PeerMessagePostRequest>,
) -> Result<Json<PeerMessage>, (StatusCode, String)> {
    if !is_local_request(addr.ip()) {
        authorize_remote(&state, &headers)?;
    }

    let mut message = build_inbound_message(body, "unknown", "Peer")
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

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
                // Keep attachment payload only if import failed; drop bulky body after success.
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

    let stored = append_message(message)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(stored))
}

async fn read_peer_message(
    State(_state): State<PeerMessageApiState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Path(message_id): Path<String>,
) -> Result<Json<PeerMessage>, (StatusCode, String)> {
    require_local(addr.ip())?;
    mark_read(&message_id).map(Json).map_err(|err| {
        if err.to_string().contains("not found") {
            (StatusCode::NOT_FOUND, err.to_string())
        } else {
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        }
    })
}

fn require_local(ip: IpAddr) -> Result<(), (StatusCode, String)> {
    if is_local_request(ip) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "Peer inbox listing is only available on the local workshop".to_string(),
        ))
    }
}

fn authorize_remote(
    state: &PeerMessageApiState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
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
    let authorized = pairing
        .authorize_bearer_token(token)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    if !authorized {
        return Err((
            StatusCode::UNAUTHORIZED,
            "Invalid or expired peer session token".to_string(),
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
