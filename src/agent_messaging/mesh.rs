//! Signed, workshop-qualified delivery into the native agent inbox.
use super::{
    MessageHost, Target, resolve_target,
    store::{Message, MessageStatus, MessageStore, TransportRoute},
    targets,
};
use crate::{
    daemon::route_policy::{
        BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
    },
    mesh::{
        CAP_MESH_MESSAGE, DEFAULT_ENVELOPE_TTL_SECS, MESH_ENVELOPE_HEADER, MeshApiState,
        MeshCapability, MeshEnvelopedRequest, MeshPeerRecord, encode_envelope_header,
        payload_hash_hex, record_has_capability, sign_envelope, verify_enveloped_payload,
    },
    pairing::PairedDeviceRecord,
    request_principal::{Capability, RequestPrincipal},
};
use anyhow::{Result, bail};
use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const PATH: &str = "/v1/mesh/agents/messages";
const SCHEMA_VERSION: u32 = 1;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Request {
    Discover {
        query: Option<String>,
    },
    Deliver {
        message: Box<WireMessage>,
    },
    Status {
        message_id: String,
        source_session: String,
        acknowledgments: Vec<Receipt>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireRequest {
    pub schema_version: u32,
    pub request: Request,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireResponse {
    pub schema_version: u32,
    pub request_hash: String,
    pub participants: Option<Vec<Target>>,
    pub receipt: Option<Receipt>,
    #[serde(default)]
    pub replies: Vec<WireMessage>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub id: String,
    pub source_session: String,
    pub target_session: String,
    pub status: MessageStatus,
    pub error: Option<String>,
}
impl From<Message> for Receipt {
    fn from(message: Message) -> Self {
        Self {
            id: message.id,
            source_session: message.source_session,
            target_session: message.target_session,
            status: message.status,
            error: message.error,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireMessage {
    id: String,
    source_session: String,
    source_turn: String,
    source_name: String,
    target_session: String,
    target_name: String,
    source_workshop: String,
    target_workshop: String,
    body: String,
    root_id: String,
    in_reply_to: Option<String>,
    created_at: DateTime<Utc>,
    hop: usize,
    #[serde(default)]
    mailbox_return: bool,
}
impl WireMessage {
    pub fn from_message(message: &Message) -> Result<Self> {
        Ok(Self {
            id: message.id.clone(),
            source_session: message.source_session.clone(),
            source_turn: message.source_turn.clone(),
            source_name: message.source_name.clone(),
            target_session: message.target_session.clone(),
            target_name: message.target_name.clone(),
            body: message.body.clone(),
            root_id: message.root_id.clone(),
            in_reply_to: message.in_reply_to.clone(),
            created_at: message.created_at,
            hop: message.hop,
            mailbox_return: matches!(
                &message.transport_route,
                Some(TransportRoute::Connection(_))
            ),
            source_workshop: message
                .source_workshop
                .clone()
                .ok_or_else(|| anyhow::anyhow!("source workshop missing"))?,
            target_workshop: message
                .target_workshop
                .clone()
                .ok_or_else(|| anyhow::anyhow!("target workshop missing"))?,
        })
    }
    fn validate(&self, sender: &str, recipient: &str) -> Result<()> {
        if self.source_workshop != sender
            || self.target_workshop != recipient
            || sender == recipient
        {
            bail!("message workshops do not match the signed transport");
        }
        for id in [&self.id, &self.root_id]
            .into_iter()
            .chain(self.in_reply_to.iter())
        {
            if !id
                .strip_prefix("msg_")
                .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                bail!("invalid message identity");
            }
        }
        crate::session_storage::SessionId::parse(&self.source_session)?;
        crate::session_storage::SessionId::parse(&self.target_session)?;
        if self.source_turn.is_empty()
            || self.source_turn.len() > 160
            || self.source_name.len() > 1024
            || self.target_name.len() > 1024
            || self.body.trim().is_empty()
            || self.body.len() > 8000
            || (self.hop == 0 && self.root_id != self.id)
        {
            bail!("message exceeds the native exchange limits");
        }
        Ok(())
    }
    fn into_message(self, owner: String) -> Message {
        Message {
            id: self.id,
            profile_id: owner,
            source_session: self.source_session,
            source_turn: self.source_turn,
            source_name: self.source_name,
            target_session: self.target_session,
            target_name: self.target_name,
            body: self.body,
            root_id: self.root_id,
            in_reply_to: self.in_reply_to,
            created_at: self.created_at,
            hop: self.hop,
            source_workshop: Some(self.source_workshop),
            target_workshop: Some(self.target_workshop),
            remote_inbound: true,
            receipt_acknowledged: false,
            transport_route: self.mailbox_return.then_some(TransportRoute::Mailbox),
            status: MessageStatus::Queued,
            delivery_turn: None,
            error: None,
            attempts: 0,
            next_attempt_at: None,
            observed_phase: None,
            wake_source: None,
        }
    }
}

#[derive(Clone)]
pub(super) struct Workshop {
    pub id: String,
    pub label: String,
    pub connection_id: Option<String>,
    peer: Option<MeshPeerRecord>,
}
pub(super) fn available_workshops(host: &MessageHost) -> Result<Vec<Workshop>> {
    let mut result: Vec<_> = host
        .state
        .daemon_workers
        .list()?
        .into_iter()
        .map(|connection| Workshop {
            id: connection.workshop_device_id,
            label: connection.label,
            connection_id: Some(connection.id),
            peer: None,
        })
        .collect();
    for peer in crate::mesh::registry::list_peers()?
        .into_iter()
        .filter(|peer| {
            peer.mesh_enabled
                && peer.endpoints.lan_base_url.is_some()
                && peer
                    .mesh_grants
                    .iter()
                    .any(|grant| grant == CAP_MESH_MESSAGE)
        })
    {
        if !result.iter().any(|workshop| workshop.id == peer.device_id) {
            result.push(Workshop {
                id: peer.device_id.clone(),
                label: peer.display_name.clone(),
                connection_id: None,
                peer: Some(peer),
            });
        }
    }
    Ok(result)
}
pub(super) fn source_workshop(host: &MessageHost, workshop: &Workshop) -> Result<String> {
    if let Some(connection) = &workshop.connection_id {
        return Ok(host
            .state
            .daemon_workers
            .agent_message_route(connection, host.client.clone())?
            .source_id()
            .to_string());
    }
    Ok(host.mesh.local_device_id.clone())
}
pub(super) fn select_participant(mut targets: Vec<Target>, selector: &str) -> Result<Target> {
    targets.retain(|target| {
        target.target_id == selector
            || target.session_id.as_deref() == Some(selector)
            || target.name.eq_ignore_ascii_case(selector)
    });
    if targets.iter().any(|target| {
        target.target_id == selector || target.session_id.as_deref() == Some(selector)
    }) {
        targets.retain(|target| {
            target.target_id == selector || target.session_id.as_deref() == Some(selector)
        });
    }
    if targets.len() != 1 {
        bail!(
            "remote agent name is missing or ambiguous; discover participants and use an exact chat or Bot ID"
        );
    }
    let target = targets.remove(0);
    if !target.can_message || target.session_id.is_none() {
        bail!("remote participant has no open native chat");
    }
    Ok(target)
}
pub(super) fn resolve_workshop(host: &MessageHost, selector: &str) -> Result<Workshop> {
    let workshops = available_workshops(host)?;
    if let Some(workshop) = workshops.iter().find(|workshop| {
        workshop.id == selector || workshop.connection_id.as_deref() == Some(selector)
    }) {
        return Ok(workshop.clone());
    }
    let mut matches: Vec<_> = workshops
        .into_iter()
        .filter(|workshop| workshop.label.eq_ignore_ascii_case(selector.trim()))
        .collect();
    if matches.len() != 1 {
        bail!(
            "workshop name is missing or ambiguous; discover workshops and use an exact workshop ID"
        );
    }
    Ok(matches.remove(0))
}
fn saved_workshop(host: &MessageHost, message: &Message) -> Result<Option<Workshop>> {
    let target = message
        .target_workshop
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("outbound workshop missing"))?;
    if matches!(message.transport_route, Some(TransportRoute::Mailbox)) {
        return Ok(None);
    }
    if let Some(TransportRoute::Connection(route)) = &message.transport_route {
        return Ok(available_workshops(host)?.into_iter().find(|workshop| {
            workshop.id == target && workshop.connection_id.as_deref() == Some(route)
        }));
    }
    Ok(crate::mesh::registry::list_peers()?
        .into_iter()
        .find(|peer| {
            peer.device_id == target
                && peer.mesh_enabled
                && peer.endpoints.lan_base_url.is_some()
                && peer
                    .mesh_grants
                    .iter()
                    .any(|grant| grant == CAP_MESH_MESSAGE)
        })
        .map(|peer| Workshop {
            id: peer.device_id.clone(),
            label: peer.display_name.clone(),
            connection_id: None,
            peer: Some(peer),
        }))
}
fn paired_sender(state: &MeshApiState, sender: &str) -> Result<PairedDeviceRecord> {
    let pairing = state
        .pairing
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("workshop pairing unavailable"))?;
    let record = pairing
        .find_by_phone_id(sender)?
        .ok_or_else(|| anyhow::anyhow!("workshop is no longer paired"))?;
    if record.phone_id != sender
        || !record_has_capability(&record, CAP_MESH_MESSAGE)
        || record.trust_expires_at.is_some_and(|at| at <= Utc::now())
    {
        bail!("workshop message trust is unavailable or expired");
    }
    if let Some(peer) = crate::mesh::registry::get_peer(sender)?
        && (peer.device_id != sender || !peer.mesh_enabled)
    {
        bail!("workshop mesh is disabled");
    }
    Ok(record)
}
fn require_targeting(state: &MeshApiState, sender: &PairedDeviceRecord) -> Result<()> {
    if sender.role.allows_full_portal() {
        return Ok(());
    }
    let policy = state
        .execution_policies
        .policy_for_peer(
            &sender.phone_id,
            &sender.pairing_id,
            record_has_capability(sender, crate::mesh::CAP_TASK_REQUEST),
        )?
        .policy;
    if !policy.permits_unattended_agent_launch(Utc::now()) {
        bail!("paired workshop policy does not allow unattended Assistant agent targeting");
    }
    Ok(())
}
fn destination_owner(sender: &PairedDeviceRecord) -> String {
    sender
        .profile_id
        .clone()
        .unwrap_or_else(crate::user_profiles::resolve_workshop_identity_user_id)
}
/// New conversations need targeting policy; saved replies only need the still
/// active message pairing and the immutable outbound return route.
pub(super) fn may_receive(host: &MessageHost, message: &Message) -> bool {
    if !message.remote_inbound {
        return true;
    }
    let Some(sender) = message.source_workshop.as_deref() else {
        return false;
    };
    if let Some(TransportRoute::Connection(route)) = &message.transport_route {
        return message.in_reply_to.is_some()
            && host.state.daemon_workers.list().is_ok_and(|connections| {
                connections.iter().any(|connection| {
                    &connection.id == route && connection.workshop_device_id == sender
                })
            });
    }
    let state = &host.mesh;
    paired_sender(state, sender).is_ok_and(|sender| {
        message.in_reply_to.is_some()
            || (message.profile_id == destination_owner(&sender)
                && require_targeting(state, &sender).is_ok())
    })
}

pub(crate) fn surface() -> DeclaredRouter<MeshApiState> {
    DeclaredRouter::default().route(
        RoutePolicy {
            method: axum::http::Method::POST,
            path: PATH,
            group: RouteGroup::PeerExchange,
            required_capability: Some(Capability::PeerExchange),
            bootstrap_public: false,
            browser_policy: BrowserPolicy::NativeOnly,
            body_limit: 64 * 1024,
            rate_limit_class: RateLimitClass::PeerExchange,
        },
        post(exchange),
    )
}
fn http_error(error: impl Into<anyhow::Error>) -> (StatusCode, String) {
    let error = error.into();
    // A journal or admission failure may happen after the message was saved.
    // Keep its identity retryable instead of treating it as a policy denial.
    let status = if error.downcast_ref::<rusqlite::Error>().is_some()
        || error.downcast_ref::<std::io::Error>().is_some()
        || error.downcast_ref::<medousa_forge::ForgeError>().is_some()
    {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::FORBIDDEN
    };
    (status, error.to_string())
}
async fn exchange(
    State(state): State<MeshApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(wrapped): Json<MeshEnvelopedRequest<WireRequest>>,
) -> Result<Response, (StatusCode, String)> {
    let host = super::HOST.get().cloned().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "native agent inbox unavailable".into(),
        )
    })?;
    let sender_id = wrapped.envelope.sender_device_id.clone();
    let sender = host
        .stored({
            let state = state.clone();
            move |_| paired_sender(&state, &sender_id)
        })
        .await
        .map_err(http_error)?;
    if principal.credential_id().map(|id| id.as_str()) != Some(sender.pairing_id.as_str()) {
        return Err((
            StatusCode::UNAUTHORIZED,
            "signed sender must match the authenticated pairing".into(),
        ));
    }
    verify_enveloped_payload(
        &wrapped,
        &sender.phone_public_key,
        &sender.phone_id,
        &state.local_device_id,
        MeshCapability::Message,
        true,
    )
    .map_err(|error| (StatusCode::UNAUTHORIZED, error.to_string()))?;
    if wrapped.payload.schema_version != SCHEMA_VERSION {
        return Err((
            StatusCode::BAD_REQUEST,
            "unsupported agent message schema".into(),
        ));
    }
    let request_hash = payload_hash_hex(&wrapped.payload).map_err(http_error)?;
    let wake_recipient = matches!(&wrapped.payload.request, Request::Deliver { .. });
    let wire = wrapped.payload;
    let response = host
        .stored({
            let state = state.clone();
            let sender = sender.clone();
            move |store| process_request(store, &state, &sender, wire, request_hash)
        })
        .await
        .map_err(http_error)?;
    if wake_recipient {
        host.wake.notify_one();
    }
    let pairing = state.pairing.clone().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "workshop pairing unavailable".into(),
        )
    })?;
    let signed = host
        .stored(move |_| {
            let hash = payload_hash_hex(&response)?;
            let seq = crate::mesh::registry::allocate_outbound_seq(&sender.phone_id)?;
            let envelope = sign_envelope(
                pairing.identity().signing_key(),
                pairing.device_id(),
                &sender.phone_id,
                seq,
                MeshCapability::Message,
                &hash,
                chrono::Duration::seconds(DEFAULT_ENVELOPE_TTL_SECS),
            );
            Ok(MeshEnvelopedRequest {
                envelope,
                payload: response,
            })
        })
        .await
        .map_err(http_error)?;
    Ok(Json(signed).into_response())
}
fn process_request(
    store: &MessageStore,
    state: &MeshApiState,
    sender: &PairedDeviceRecord,
    wire: WireRequest,
    request_hash: String,
) -> Result<WireResponse> {
    let mut response = WireResponse {
        schema_version: SCHEMA_VERSION,
        request_hash,
        participants: None,
        receipt: None,
        replies: Vec::new(),
    };
    match wire.request {
        Request::Discover { query } => {
            require_targeting(state, sender)?;
            if query.as_ref().is_some_and(|q| q.len() > 160) {
                bail!("participant query exceeds 160 bytes");
            }
            let owner = destination_owner(sender);
            response.participants = Some(if let Some(query) = query {
                vec![resolve_target(&owner, &query)?]
            } else {
                let mut participants = targets(&owner, None)?;
                participants.truncate(200);
                participants
            });
        }
        Request::Status {
            message_id,
            source_session,
            acknowledgments,
        } => {
            let message = store
                .get(&message_id)?
                .ok_or_else(|| anyhow::anyhow!("remote message unavailable"))?;
            if !message.remote_inbound
                || message.source_workshop.as_deref() != Some(sender.phone_id.as_str())
                || message.target_workshop.as_deref() != Some(state.local_device_id.as_str())
                || message.source_session != source_session
            {
                bail!("message status is unavailable to this return route");
            }
            append_mailbox(store, &message, &acknowledgments, &mut response)?;
            response.receipt = Some(message.into());
        }
        Request::Deliver { message } => {
            message.validate(&sender.phone_id, &state.local_device_id)?;
            let owner = if let Some(id) = &message.in_reply_to {
                store
                    .get(id)?
                    .ok_or_else(|| anyhow::anyhow!("reply has no saved outbound route"))?
                    .profile_id
            } else {
                require_targeting(state, sender)?;
                destination_owner(sender)
            };
            let target = resolve_target(&owner, &message.target_session)?;
            if !target.can_message
                || !crate::session_history::session_visible_to_profile(
                    &message.target_session,
                    &owner,
                )
            {
                bail!("recipient is unavailable to this owner");
            }
            let accepted = store.accept_remote(message.into_message(owner))?;
            append_mailbox(store, &accepted, &[], &mut response)?;
            response.receipt = Some(accepted.into());
        }
    }
    if serde_json::to_vec(&response)?.len() > MAX_RESPONSE_BYTES {
        bail!("agent message response exceeds its byte budget");
    }
    Ok(response)
}
fn append_mailbox(
    store: &MessageStore,
    incoming: &Message,
    acknowledgments: &[Receipt],
    response: &mut WireResponse,
) -> Result<()> {
    if acknowledgments.len() > 16 {
        bail!("reply acknowledgment budget exceeded");
    }
    for ack in acknowledgments {
        let sent = store
            .get(&ack.id)?
            .ok_or_else(|| anyhow::anyhow!("reply acknowledgment unavailable"))?;
        if sent.remote_inbound
            || sent.target_workshop != incoming.source_workshop
            || sent.source_workshop != incoming.target_workshop
            || sent.target_session != incoming.source_session
            || sent.root_id != incoming.root_id
            || ack.source_session != sent.source_session
            || ack.target_session != sent.target_session
        {
            bail!("reply acknowledgment does not match this saved mailbox route");
        }
        let status = match ack.status {
            MessageStatus::Queued | MessageStatus::Delivering => MessageStatus::Delivering,
            ref status => status.clone(),
        };
        store.outcome(&ack.id, status, ack.error.clone())?;
    }
    let replies = store.replies_for_route(
        incoming
            .source_workshop
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("mailbox source missing"))?,
        &incoming.source_session,
        &incoming.root_id,
    )?;
    for reply in replies {
        response.replies.push(WireMessage::from_message(&reply)?);
        store.outcome(&reply.id, MessageStatus::Delivering, None)?;
    }
    Ok(())
}

#[derive(Debug)]
pub(super) struct TransportError {
    pub retryable: bool,
    pub message: String,
}
impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for TransportError {}
fn transient(error: impl std::fmt::Display) -> TransportError {
    TransportError {
        retryable: true,
        message: error.to_string(),
    }
}

pub(super) async fn request(
    host: &Arc<MessageHost>,
    workshop: Workshop,
    request: Request,
) -> std::result::Result<WireResponse, TransportError> {
    let wire = WireRequest {
        schema_version: SCHEMA_VERSION,
        request,
    };
    let request_hash = payload_hash_hex(&wire).map_err(transient)?;
    if let Some(route) = workshop.connection_id {
        let workers = host.state.daemon_workers.clone();
        let client = host.client.clone();
        let prepared = host
            .stored(move |_| workers.agent_message_route(&route, client))
            .await
            .map_err(transient)?;
        if prepared.target_id() != workshop.id {
            return Err(transient("workshop connection identity changed"));
        }
        let response: WireResponse = prepared.exchange(PATH, wire).await.map_err(|error| {
            let retryable = error
                .downcast_ref::<crate::delegated_task::DelegatedTaskError>()
                .is_none_or(|error| {
                    matches!(
                        error.kind,
                        crate::delegated_task::DelegatedTaskErrorKind::Transport
                            | crate::delegated_task::DelegatedTaskErrorKind::Internal
                    )
                });
            TransportError {
                retryable,
                message: error.to_string(),
            }
        })?;
        if response.schema_version != SCHEMA_VERSION || response.request_hash != request_hash {
            return Err(transient(
                "workshop response does not match the signed request",
            ));
        }
        return Ok(response);
    }
    let peer = workshop
        .peer
        .ok_or_else(|| transient("workshop dial route unavailable"))?;
    let pairing = host
        .mesh
        .pairing
        .clone()
        .ok_or_else(|| transient("workshop pairing unavailable"))?;
    let signed = host
        .stored({
            let peer = peer.clone();
            let pairing = pairing.clone();
            let wire = wire.clone();
            let state = host.mesh.clone();
            move |_| {
                paired_sender(&state, &peer.device_id)?;
                let seq = crate::mesh::registry::allocate_outbound_seq(&peer.device_id)?;
                let hash = payload_hash_hex(&wire)?;
                let envelope = sign_envelope(
                    pairing.identity().signing_key(),
                    pairing.device_id(),
                    &peer.device_id,
                    seq,
                    MeshCapability::Message,
                    &hash,
                    chrono::Duration::seconds(DEFAULT_ENVELOPE_TTL_SECS),
                );
                Ok(MeshEnvelopedRequest {
                    envelope,
                    payload: wire,
                })
            }
        })
        .await
        .map_err(transient)?;
    post_signed(
        &host.client,
        &peer,
        pairing.device_id(),
        signed,
        &request_hash,
    )
    .await
}
async fn post_signed(
    client: &reqwest::Client,
    peer: &MeshPeerRecord,
    local: &str,
    signed: MeshEnvelopedRequest<WireRequest>,
    request_hash: &str,
) -> std::result::Result<WireResponse, TransportError> {
    let base = peer
        .endpoints
        .lan_base_url
        .as_deref()
        .ok_or_else(|| transient("workshop has no dial endpoint"))?
        .trim_end_matches('/');
    let header = encode_envelope_header(&signed.envelope).map_err(transient)?;
    let mut response = client
        .post(format!("{base}{PATH}"))
        .header(MESH_ENVELOPE_HEADER, header)
        .json(&signed)
        .send()
        .await
        .map_err(transient)?;
    let status = response.status();
    if !status.is_success() {
        return Err(TransportError {
            retryable: status.is_server_error()
                || status == StatusCode::TOO_MANY_REQUESTS
                || status == StatusCode::REQUEST_TIMEOUT,
            message: format!("workshop message exchange HTTP {status}"),
        });
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transient)? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(transient("workshop response exceeds 256 KiB"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let wrapped: MeshEnvelopedRequest<WireResponse> =
        serde_json::from_slice(&bytes).map_err(transient)?;
    verify_enveloped_payload(
        &wrapped,
        &peer.public_key_b64,
        &peer.device_id,
        local,
        MeshCapability::Message,
        true,
    )
    .map_err(transient)?;
    if wrapped.payload.schema_version != SCHEMA_VERSION
        || wrapped.payload.request_hash != request_hash
    {
        return Err(transient(
            "workshop response does not match the signed request",
        ));
    }
    Ok(wrapped.payload)
}

pub(super) async fn deliver(host: &Arc<MessageHost>, id: String) -> Result<()> {
    let message = host
        .stored({
            let id = id.clone();
            move |store| {
                store
                    .get(&id)?
                    .ok_or_else(|| anyhow::anyhow!("outbound message unavailable"))
            }
        })
        .await?;
    let host_lookup = host.clone();
    let route_message = message.clone();
    let peer = host
        .stored(move |_| saved_workshop(&host_lookup, &route_message))
        .await?;
    let Some(peer) = peer else {
        if !matches!(message.transport_route, Some(TransportRoute::Mailbox))
            && (message.in_reply_to.is_none()
                || matches!(message.transport_route, Some(TransportRoute::Connection(_))))
        {
            host.stored(move |store| {
                let status = if message.status == MessageStatus::Delivered {
                    MessageStatus::Delivered
                } else {
                    MessageStatus::Failed
                };
                store.outcome(
                    &id,
                    status,
                    Some("saved workshop connection is no longer available".into()),
                )
            })
            .await?;
        }
        return Ok(());
    }; // Directional links return replies through polling.
    let wire = WireMessage::from_message(&message)?;
    let previous = message.clone();
    let receipt_messages = if message.status != MessageStatus::Queued {
        host.stored(move |store| store.received_reply_outcomes(&previous))
            .await?
    } else {
        Vec::new()
    };
    let request_body = if message.status != MessageStatus::Queued {
        let acknowledgments = receipt_messages
            .iter()
            .cloned()
            .map(Receipt::from)
            .collect();
        Request::Status {
            message_id: id.clone(),
            source_session: message.source_session.clone(),
            acknowledgments,
        }
    } else {
        Request::Deliver {
            message: Box::new(wire),
        }
    };
    let result = request(host, peer, request_body).await;
    match result {
        Ok(response) => {
            let receipt = response
                .receipt
                .ok_or_else(|| anyhow::anyhow!("workshop message receipt missing"))?;
            if receipt.id != id
                || receipt.source_session != message.source_session
                || receipt.target_session != message.target_session
                || response.replies.len() > 2
            {
                bail!("workshop receipt does not match the outbound message");
            }
            let previous = message.clone();
            let imported = host
                .stored(move |store| {
                    let mut imported = false;
                    for reply in response.replies {
                        reply.validate(
                            previous.target_workshop.as_deref().unwrap_or_default(),
                            previous.source_workshop.as_deref().unwrap_or_default(),
                        )?;
                        let parent_id = reply
                            .in_reply_to
                            .as_deref()
                            .ok_or_else(|| anyhow::anyhow!("pulled message is not a reply"))?;
                        let parent = store.get(parent_id)?.ok_or_else(|| {
                            anyhow::anyhow!("pulled reply has no saved return route")
                        })?;
                        let incoming = reply.into_message(parent.profile_id.clone());
                        if incoming.source_workshop != previous.target_workshop
                            || parent.transport_route != previous.transport_route
                            || incoming.root_id != previous.root_id
                        {
                            bail!("pulled reply belongs to another workshop route");
                        }
                        imported |= store.get(&incoming.id)?.is_none();
                        store.accept_remote(incoming)?;
                    }
                    let status = match receipt.status {
                        MessageStatus::Queued | MessageStatus::Delivering => {
                            MessageStatus::Delivering
                        }
                        status => status,
                    };
                    store.acknowledge_reply_outcomes(&receipt_messages)?;
                    store.outcome(&id, status, receipt.error)?;
                    Ok(imported)
                })
                .await?;
            if imported {
                host.wake.notify_one();
            }
        }
        Err(error) => {
            host.stored(move |store| {
                store.outcome(
                    &id,
                    if error.retryable {
                        message.status
                    } else {
                        MessageStatus::Failed
                    },
                    Some(error.message),
                )
            })
            .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_failure_is_retryable_but_policy_denial_is_terminal() {
        assert_eq!(
            http_error(std::io::Error::other("journal temporarily unavailable")).0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            http_error(rusqlite::Error::InvalidQuery).0,
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            http_error(anyhow::anyhow!("agent targeting denied")).0,
            StatusCode::FORBIDDEN
        );
    }

    fn outgoing(from: &str, to: &str, turn: &str, owner: &str) -> Message {
        Message {
            id: String::new(),
            profile_id: owner.into(),
            source_session: "ses_coordinator".into(),
            source_turn: turn.into(),
            source_name: "Medousa".into(),
            target_session: "ses_taco".into(),
            target_name: "Taco".into(),
            body: "Please review the API".into(),
            root_id: String::new(),
            in_reply_to: None,
            created_at: Utc::now(),
            status: MessageStatus::Queued,
            delivery_turn: None,
            error: None,
            source_workshop: Some(from.into()),
            target_workshop: Some(to.into()),
            hop: 0,
            remote_inbound: false,
            receipt_acknowledged: false,
            transport_route: Some(TransportRoute::Connection("worker-b".into())),
            attempts: 0,
            next_attempt_at: None,
            observed_phase: None,
            wake_source: None,
        }
    }
    fn response() -> WireResponse {
        WireResponse {
            schema_version: SCHEMA_VERSION,
            request_hash: "test".into(),
            participants: None,
            receipt: None,
            replies: Vec::new(),
        }
    }
    fn receive(store: &MessageStore, sent: &Message, owner: &str) -> Message {
        let wire = WireMessage::from_message(sent).unwrap();
        wire.validate(
            sent.source_workshop.as_deref().unwrap(),
            sent.target_workshop.as_deref().unwrap(),
        )
        .unwrap();
        store
            .accept_remote(wire.into_message(owner.into()))
            .unwrap()
    }
    fn reply(store: &MessageStore, received: &Message, turn: &str) -> Message {
        let mut reply = outgoing("b", "a", turn, &received.profile_id);
        reply.source_session = received.target_session.clone();
        reply.target_session = received.source_session.clone();
        reply.source_name = received.target_name.clone();
        reply.target_name = received.source_name.clone();
        reply.in_reply_to = Some(received.id.clone());
        reply.source_workshop = received.target_workshop.clone();
        reply.target_workshop = received.source_workshop.clone();
        reply.transport_route = received.transport_route.clone();
        store.enqueue(reply, "reply").unwrap()
    }

    #[test]
    fn directional_mailbox_survives_lost_response_restart_and_acknowledgment() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let source = MessageStore::new(a.path().join("messages.sqlite3"));
        let destination = MessageStore::new(b.path().join("messages.sqlite3"));
        let sent = source
            .enqueue(outgoing("worker-a", "workshop-b", "t", "alice"), "send")
            .unwrap();
        // The remote profile comes from destination ownership, never the wire.
        let received = receive(&destination, &sent, "bob");
        assert_eq!(received.profile_id, "bob");
        destination.claim(&received.id).unwrap().unwrap();
        assert!(receive(&destination, &sent, "bob").delivery_turn.is_some());
        assert!(destination.claim(&received.id).unwrap().is_none());
        let answer = reply(&destination, &received, "reply-turn");
        assert_eq!(answer.transport_route, Some(TransportRoute::Mailbox)); // no reverse dial
        let mut first_poll = response();
        append_mailbox(&destination, &received, &[], &mut first_poll).unwrap();
        let restarted = MessageStore::new(b.path().join("messages.sqlite3"));
        let mut retry = response();
        append_mailbox(&restarted, &received, &[], &mut retry).unwrap();
        assert_eq!(retry.replies[0].id, first_poll.replies[0].id);
        let returned = source
            .accept_remote(retry.replies.remove(0).into_message("alice".into()))
            .unwrap();
        assert_eq!(returned.transport_route, sent.transport_route);
        assert_eq!(returned.hop, 1);
        assert_eq!(returned.root_id, sent.root_id);
        assert!(
            source
                .inbox("bob", "ses_coordinator", "active")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            source
                .inbox("alice", "ses_coordinator", "active")
                .unwrap()
                .len(),
            1
        );
        assert!(receive(&source, &answer, "alice").delivery_turn.is_some());
        assert!(source.claim(&answer.id).unwrap().is_none());
        let acknowledgments: Vec<_> = source
            .received_reply_outcomes(&sent)
            .unwrap()
            .into_iter()
            .map(Receipt::from)
            .collect();
        let mut acknowledged = response();
        append_mailbox(&restarted, &received, &acknowledgments, &mut acknowledged).unwrap();
        assert!(acknowledged.replies.is_empty());
        assert_eq!(
            restarted.get(&answer.id).unwrap().unwrap().status,
            MessageStatus::Delivered
        );
        // The original route remains available for replies after initial delivery.
        source
            .outcome(&sent.id, MessageStatus::Delivered, None)
            .unwrap();
        assert_eq!(source.outbound_routes(0).unwrap()[0].1, sent.id);
    }

    #[test]
    fn remote_routes_bind_owner_workshops_chats_and_immutable_content() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let source = MessageStore::new(a.path().join("messages.sqlite3"));
        let destination = MessageStore::new(b.path().join("messages.sqlite3"));
        let mut input = outgoing("a", "b", "t", "alice");
        input.target_session = input.source_session.clone(); // equal IDs on different workshops
        let sent = source.enqueue(input, "send").unwrap();
        assert!(
            source
                .inbox("alice", &sent.source_session, "active")
                .unwrap()
                .is_empty()
        );
        let received = receive(&destination, &sent, "bob");
        let mut altered = received.clone();
        altered.body.push_str(" changed");
        assert!(destination.accept_remote(altered).is_err());
        let answer = reply(&destination, &received, "r");
        let returned = WireMessage::from_message(&answer)
            .unwrap()
            .into_message("alice".into());
        for mutation in 0..5 {
            let mut forged = returned.clone();
            match mutation {
                0 => forged.profile_id = "mallory".into(),
                1 => forged.source_workshop = Some("c".into()),
                2 => forged.target_session = "ses_other".into(),
                3 => forged.hop = 0,
                _ => forged.root_id = format!("msg_{}", "f".repeat(64)),
            }
            assert!(source.accept_remote(forged).is_err());
        }
        let mut bad_ack = Receipt::from(answer.clone());
        bad_ack.target_session = "ses_other".into();
        assert!(append_mailbox(&destination, &received, &[bad_ack], &mut response()).is_err());
        assert!(source.accept_remote(returned).is_ok());
        let wire = WireMessage::from_message(&sent).unwrap();
        assert!(wire.validate("c", "b").is_err());
        assert!(wire.validate("a", "c").is_err());
        let payload = serde_json::to_value(wire).unwrap();
        for private in [
            "profile_id",
            "transport_route",
            "delivery_turn",
            "receipt_acknowledged",
            "session_token",
        ] {
            assert!(payload.get(private).is_none());
        }
    }

    #[test]
    fn mailbox_acknowledges_all_imported_replies_without_starving_older_ones() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let source = MessageStore::new(a.path().join("messages.sqlite3"));
        let destination = MessageStore::new(b.path().join("messages.sqlite3"));
        let sent = source
            .enqueue(outgoing("a", "b", "t", "alice"), "send")
            .unwrap();
        let received = receive(&destination, &sent, "bob");
        for n in 0..40 {
            let answer = reply(&destination, &received, &format!("r-{n}"));
            receive(&source, &answer, "alice");
        }
        for _ in 0..5 {
            source.inbox("alice", "ses_coordinator", "active").unwrap();
        }
        let mut count = 0;
        loop {
            let receipts = source.received_reply_outcomes(&sent).unwrap();
            if receipts.is_empty() {
                break;
            }
            assert!(receipts.len() <= 16); // Wire page size, never an exchange allowance.
            count += receipts.len();
            let acknowledgments = receipts
                .iter()
                .cloned()
                .map(Receipt::from)
                .collect::<Vec<_>>();
            let mut result = response();
            append_mailbox(&destination, &received, &acknowledgments, &mut result).unwrap();
            source.acknowledge_reply_outcomes(&receipts).unwrap();
        }
        assert_eq!(count, 40);
        let mut result = response();
        append_mailbox(&destination, &received, &[], &mut result).unwrap();
        assert!(result.replies.is_empty());
    }

    #[test]
    fn distributed_exchanges_continue_beyond_sixteen_messages() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let source = MessageStore::new(a.path().join("messages.sqlite3"));
        let destination = MessageStore::new(b.path().join("messages.sqlite3"));
        let mut sent = source
            .enqueue(outgoing("a", "b", "t", "alice"), "send")
            .unwrap();
        for n in 1..=40 {
            let (store, owner) = if n % 2 == 1 {
                (&destination, "bob")
            } else {
                (&source, "alice")
            };
            let received = receive(store, &sent, owner);
            store
                .inbox(owner, &received.target_session, &format!("consume-{n}"))
                .unwrap();
            sent = reply(store, &received, &format!("r-{n}"));
            assert_eq!(sent.hop, n);
            WireMessage::from_message(&sent)
                .unwrap()
                .validate(
                    sent.source_workshop.as_deref().unwrap(),
                    sent.target_workshop.as_deref().unwrap(),
                )
                .unwrap();
        }
    }

    #[test]
    fn participant_names_are_ambiguous_but_exact_ids_win() {
        let target = |id: &str| Target {
            target_id: id.into(),
            session_id: Some(id.into()),
            name: "Taco".into(),
            kind: "chat".into(),
            can_message: true,
            workshop_id: None,
        };
        let participants = vec![target("ses_one"), target("ses_two")];
        assert!(select_participant(participants.clone(), "taco").is_err());
        assert_eq!(
            select_participant(participants, "ses_one")
                .unwrap()
                .target_id,
            "ses_one"
        );
        let mut archived = target("ses_archived");
        archived.can_message = false;
        assert!(select_participant(vec![archived], "ses_archived").is_err());
    }

    #[test]
    fn peer_discovery_and_wakes_require_current_agent_targeting_policy() {
        use crate::peer_execution_policy::{
            PeerExecutionPolicyPreset, PeerExecutionPolicyStore, PeerExecutionPolicyUpdate,
        };
        let directory = tempfile::tempdir().unwrap();
        let state = MeshApiState {
            pairing: None,
            local_device_id: "b".into(),
            execution_policies: Arc::new(PeerExecutionPolicyStore::new(
                directory.path().join("policies.json"),
            )),
            delegated_task_executor: None,
            computer_drivers: Arc::new(crate::computer_driver::ComputerDriverBroker::new(
                Arc::new(crate::world_authority::WorldAuthorityService::default()),
            )),
            isolated_browser_available: false,
        };
        let sender: PairedDeviceRecord = serde_json::from_value(serde_json::json!({
            "pairingId":"pair-a", "phoneId":"a", "phoneName":"A", "phonePublicKey":"test",
            "pairedAt":Utc::now(), "lastSeen":Utc::now(), "sessionTokenHash":"test",
            "sessionTokenExpiry":Utc::now()+chrono::Duration::hours(1), "role":"peer"
        }))
        .unwrap();
        assert!(require_targeting(&state, &sender).is_err());
        let mut bound = sender.clone();
        bound.profile_id = Some("user:bob".into());
        assert_eq!(destination_owner(&bound), "user:bob");
        state
            .execution_policies
            .update_policy(
                "a",
                "pair-a",
                PeerExecutionPolicyUpdate {
                    preset: PeerExecutionPolicyPreset::Custom,
                    assistant_work: Some(true),
                    allow_agent_targeting: Some(true),
                    host_shell: Some(true),
                    ..Default::default()
                },
                "operator",
            )
            .unwrap();
        assert!(require_targeting(&state, &sender).is_ok());
        state
            .execution_policies
            .update_policy(
                "a",
                "pair-a",
                PeerExecutionPolicyUpdate {
                    preset: PeerExecutionPolicyPreset::Custom,
                    assistant_work: Some(true),
                    allow_agent_targeting: Some(false),
                    host_shell: Some(true),
                    ..Default::default()
                },
                "operator",
            )
            .unwrap();
        assert!(require_targeting(&state, &sender).is_err());
    }
}
