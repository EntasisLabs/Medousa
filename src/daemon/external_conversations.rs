//! Durable conversations with provider-hosted agents. Transport acknowledgments
//! and agent outcomes are separate events; no ACP process is created here.

pub mod access;
pub(crate) mod work_dispatch;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{ConnectInfo, Extension, Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::routing::{delete, get, post};
use chrono::{DateTime, TimeDelta, Utc};
use medousa_secrets::{
    delete_daemon_secret, ensure_installation_id, load_daemon_secret, save_daemon_secret,
};
use medousa_types::secrets::{ConnectionId, DaemonSecretPath, IntegrationSecretSlot};
use medousa_types::{
    CreateExternalConversationRequest as CreateConversationRequest,
    CreateExternalConversationResponse as CreateConversationResponse,
    DeleteExternalConversationResponse, ExternalConversationEvent as ConversationEvent,
    ExternalConversationListResponse as ConversationListResponse,
    ExternalConversationSendRequest as SendMessageRequest,
    ExternalConversationView as ConversationView, ExternalEventKind as EventKind,
    ExternalInboundClaimResponse as InboundClaimResponse,
    ExternalMuseDiscoveryStatus as MuseDiscoveryStatus, ExternalProvider as Provider,
    ExternalProviderEventRequest as ProviderEventRequest,
    ExternalSlackInboundRequest as SlackInboundRequest,
    ExternalWhatsAppInboundRequest as WhatsAppInboundRequest,
    ExternalWhatsAppPairingState as PairingState, ExternalWhatsAppPairingStatus as PairingStatus,
    ExternalWhatsAppPairingUpdateRequest as PairingUpdateRequest, MessageReaction,
    RotateExternalCallbackResponse,
};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::daemon::state::AppState;
use crate::request_principal::{Capability, RequestPrincipal};

type HttpError = (StatusCode, String);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ConversationRecord {
    id: String,
    owner_id: String,
    provider: Provider,
    label: String,
    target: String,
    #[serde(default)]
    dot_user_id: Option<String>,
    webhook_url: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    events: Vec<ConversationEvent>,
    #[serde(default)]
    api_access: Option<access::StoredGrant>,
}

impl From<&ConversationRecord> for ConversationView {
    fn from(record: &ConversationRecord) -> Self {
        Self {
            id: record.id.clone(),
            provider: record.provider,
            label: record.label.clone(),
            target: record.target.clone(),
            dot_user_id: record.dot_user_id.clone(),
            created_at: record.created_at,
            updated_at: record.updated_at,
            events: record.events.clone(),
            api_access: record.api_access.as_ref().map(|grant| grant.status.clone()),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Document {
    conversations: BTreeMap<String, ConversationRecord>,
}

#[derive(Clone)]
struct MuseDiscovery {
    challenge: String,
    observed_chat_jid: Option<String>,
    expires_at: DateTime<Utc>,
}

impl From<&MuseDiscovery> for MuseDiscoveryStatus {
    fn from(discovery: &MuseDiscovery) -> Self {
        Self {
            challenge: discovery.challenge.clone(),
            observed_chat_jid: discovery.observed_chat_jid.clone(),
            expires_at: discovery.expires_at,
        }
    }
}

pub struct ExternalConversationStore {
    path: PathBuf,
    document: Mutex<Document>,
    send_locks: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
    muse_discoveries: Mutex<BTreeMap<String, MuseDiscovery>>,
    whatsapp_pairing: Mutex<PairingStatus>,
}

impl ExternalConversationStore {
    pub async fn open(path: PathBuf) -> anyhow::Result<Arc<Self>> {
        let document = match tokio::fs::read(&path).await {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Document::default(),
            Err(error) => return Err(error.into()),
        };
        Ok(Arc::new(Self {
            path,
            document: Mutex::new(document),
            send_locks: Mutex::new(BTreeMap::new()),
            muse_discoveries: Mutex::new(BTreeMap::new()),
            whatsapp_pairing: Mutex::new(PairingStatus {
                state: PairingState::Waiting,
                qr_svg: None,
                expires_at: None,
            }),
        }))
    }

    async fn send_lock(&self, id: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = self
            .send_locks
            .lock()
            .await
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        lock.lock_owned().await
    }

    async fn persist(&self, next: &Document) -> anyhow::Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("missing store parent"))?;
        tokio::fs::create_dir_all(parent).await?;
        let temporary = parent.join(format!(
            ".external-conversations-{}.tmp",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::write(&temporary, serde_json::to_vec(next)?).await?;
        if let Err(error) = tokio::fs::rename(&temporary, &self.path).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(error.into());
        }
        Ok(())
    }

    pub async fn list(&self, owner_id: &str) -> Vec<ConversationView> {
        self.document
            .lock()
            .await
            .conversations
            .values()
            .filter(|record| record.owner_id == owner_id)
            .map(ConversationView::from)
            .collect()
    }

    pub async fn get(&self, owner_id: &str, id: &str) -> Option<ConversationView> {
        self.document
            .lock()
            .await
            .conversations
            .get(id)
            .filter(|record| record.owner_id == owner_id)
            .map(ConversationView::from)
    }

    async fn create(
        &self,
        id: String,
        owner_id: String,
        input: &CreateConversationRequest,
    ) -> anyhow::Result<ConversationView> {
        let mut current = self.document.lock().await;
        if matches!(input.provider, Provider::Muse | Provider::Instinct)
            && current.conversations.values().any(|record| {
                matches!(record.provider, Provider::Muse | Provider::Instinct)
                    && record.target == input.target.trim()
            })
        {
            anyhow::bail!("WhatsApp chat is already bound to a conversation");
        }
        if input.provider == Provider::Dots
            && current.conversations.values().any(|record| {
                record.provider == Provider::Dots && record.target == input.target.trim()
            })
        {
            anyhow::bail!("Slack channel is already bound to a dot conversation");
        }
        let mut next = current.clone();
        let now = Utc::now();
        let record = ConversationRecord {
            id,
            owner_id,
            provider: input.provider,
            label: input.label.trim().to_string(),
            target: input.target.trim().to_string(),
            dot_user_id: input.dot_user_id.clone(),
            webhook_url: input.webhook_url.clone(),
            created_at: now,
            updated_at: now,
            events: Vec::new(),
            api_access: None,
        };
        let view = ConversationView::from(&record);
        next.conversations.insert(record.id.clone(), record);
        self.persist(&next).await?;
        *current = next;
        Ok(view)
    }

    async fn record(
        &self,
        id: &str,
        event_id: String,
        request_id: Option<String>,
        kind: EventKind,
        text: String,
    ) -> anyhow::Result<Option<ConversationView>> {
        self.record_event(id, event_id, request_id, kind, text, None)
            .await
    }

    async fn record_reaction(
        &self,
        id: &str,
        event_id: String,
        request_id: Option<String>,
        kind: EventKind,
        text: String,
        reaction: MessageReaction,
    ) -> anyhow::Result<Option<ConversationView>> {
        self.record_event(id, event_id, request_id, kind, text, Some(reaction))
            .await
    }

    async fn record_event(
        &self,
        id: &str,
        event_id: String,
        request_id: Option<String>,
        kind: EventKind,
        text: String,
        reaction: Option<MessageReaction>,
    ) -> anyhow::Result<Option<ConversationView>> {
        let mut current = self.document.lock().await;
        let Some(record) = current.conversations.get(id) else {
            return Ok(None);
        };
        if record.events.iter().any(|event| event.event_id == event_id) {
            return Ok(Some(ConversationView::from(record)));
        }
        let mut next = current.clone();
        let record = next.conversations.get_mut(id).expect("record checked");
        record.events.push(ConversationEvent {
            sequence: record.events.len() as u64 + 1,
            event_id,
            request_id,
            kind,
            text,
            reaction,
            created_at: Utc::now(),
        });
        record.updated_at = Utc::now();
        let view = ConversationView::from(&*record);
        self.persist(&next).await?;
        *current = next;
        Ok(Some(view))
    }

    async fn binding(&self, id: &str) -> Option<ConversationRecord> {
        self.document.lock().await.conversations.get(id).cloned()
    }

    async fn remove(&self, owner_id: &str, id: &str) -> anyhow::Result<bool> {
        let mut current = self.document.lock().await;
        if current
            .conversations
            .get(id)
            .is_none_or(|record| record.owner_id != owner_id)
        {
            return Ok(false);
        }
        let mut next = current.clone();
        next.conversations.remove(id);
        self.persist(&next).await?;
        *current = next;
        Ok(true)
    }

    async fn whatsapp_binding(&self, jid: &str) -> Option<ConversationRecord> {
        self.document
            .lock()
            .await
            .conversations
            .values()
            .find(|record| {
                matches!(record.provider, Provider::Muse | Provider::Instinct)
                    && record.target == jid
            })
            .cloned()
    }

    async fn dots_binding(&self, channel_id: &str) -> Option<ConversationRecord> {
        self.document
            .lock()
            .await
            .conversations
            .values()
            .find(|record| record.provider == Provider::Dots && record.target == channel_id)
            .cloned()
    }

    async fn start_muse_discovery(&self, owner_id: String) -> MuseDiscoveryStatus {
        let discovery = MuseDiscovery {
            challenge: format!("MEDOUSA-MUSE-{}", uuid::Uuid::new_v4()),
            observed_chat_jid: None,
            expires_at: Utc::now() + TimeDelta::minutes(5),
        };
        let status = MuseDiscoveryStatus::from(&discovery);
        self.muse_discoveries
            .lock()
            .await
            .insert(owner_id, discovery);
        status
    }

    async fn muse_discovery(&self, owner_id: &str) -> Option<MuseDiscoveryStatus> {
        self.muse_discoveries
            .lock()
            .await
            .get(owner_id)
            .filter(|discovery| discovery.expires_at > Utc::now())
            .map(MuseDiscoveryStatus::from)
    }

    async fn observe_muse_challenge(&self, chat_jid: &str, text: &str) -> bool {
        if chat_jid.ends_with("@g.us") || chat_jid.trim().is_empty() {
            return false;
        }
        let mut discoveries = self.muse_discoveries.lock().await;
        let Some(discovery) = discoveries.values_mut().find(|discovery| {
            discovery.expires_at > Utc::now() && text.contains(&discovery.challenge)
        }) else {
            return false;
        };
        discovery.observed_chat_jid = Some(chat_jid.to_string());
        true
    }

    async fn clear_muse_discovery(&self, owner_id: &str) {
        self.muse_discoveries.lock().await.remove(owner_id);
    }

    async fn whatsapp_pairing(&self) -> PairingStatus {
        let mut status = self.whatsapp_pairing.lock().await;
        if status
            .expires_at
            .is_some_and(|expires_at| expires_at <= Utc::now())
        {
            *status = PairingStatus {
                state: PairingState::Waiting,
                qr_svg: None,
                expires_at: None,
            };
        }
        status.clone()
    }

    async fn update_whatsapp_pairing(
        &self,
        update: PairingUpdateRequest,
    ) -> Result<PairingStatus, String> {
        let (qr_svg, expires_at) = if update.state == PairingState::QrReady {
            let code = update
                .qr_code
                .as_deref()
                .filter(|code| !code.is_empty() && code.len() <= 2048)
                .ok_or_else(|| "invalid WhatsApp pairing QR".to_string())?;
            let seconds = update
                .expires_in_seconds
                .filter(|seconds| (1..=120).contains(seconds))
                .ok_or_else(|| "invalid WhatsApp pairing expiry".to_string())?;
            let svg = qrcode::QrCode::new(code.as_bytes())
                .map_err(|_| "invalid WhatsApp pairing QR".to_string())?
                .render::<qrcode::render::svg::Color>()
                .min_dimensions(256, 256)
                .build();
            (
                Some(svg),
                Some(Utc::now() + TimeDelta::seconds(seconds as i64)),
            )
        } else {
            if update.qr_code.is_some() || update.expires_in_seconds.is_some() {
                return Err("pairing code only allowed for QR state".into());
            }
            (None, None)
        };
        let status = PairingStatus {
            state: update.state,
            qr_svg,
            expires_at,
        };
        *self.whatsapp_pairing.lock().await = status.clone();
        Ok(status)
    }
}

fn owner(principal: &RequestPrincipal, state: &AppState) -> String {
    principal
        .profile_id()
        .map(str::to_string)
        .unwrap_or_else(|| state.workshop_identity_user_id())
}

fn bad_request(message: &str) -> HttpError {
    (StatusCode::BAD_REQUEST, message.into())
}
fn internal(error: impl std::fmt::Display) -> HttpError {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn validate_webhook_url(raw: &str) -> Result<(), HttpError> {
    let url = reqwest::Url::parse(raw).map_err(|_| bad_request("invalid webhook URL"))?;
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "https"
        || url.username() != ""
        || url.password().is_some()
        || !["cursor.sh", "cursor.com"]
            .iter()
            .any(|root| host == *root || host.ends_with(&format!(".{root}")))
        || !matches!(url.port(), None | Some(443))
    {
        return Err(bad_request(
            "Grok Bot webhook URL must use HTTPS on cursor.com or cursor.sh",
        ));
    }
    Ok(())
}

fn secret_path(id: &str, slot: IntegrationSecretSlot) -> anyhow::Result<DaemonSecretPath> {
    Ok(DaemonSecretPath::Integration {
        installation_id: ensure_installation_id(&crate::paths::medousa_data_dir())?,
        connection_id: ConnectionId::parse(id).map_err(|e| anyhow::anyhow!("{e}"))?,
        slot,
    })
}

async fn save_secret(id: String, slot: IntegrationSecretSlot, value: String) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || {
        let root = crate::paths::medousa_data_dir();
        save_daemon_secret(&root, &secret_path(&id, slot)?, &value).map(|_| ())
    })
    .await?
}

async fn load_secret(id: String, slot: IntegrationSecretSlot) -> anyhow::Result<Option<String>> {
    tokio::task::spawn_blocking(move || {
        let root = crate::paths::medousa_data_dir();
        Ok(load_daemon_secret(&root, &secret_path(&id, slot)?)?.map(|read| read.value))
    })
    .await?
}

async fn delete_secret(id: String, slot: IntegrationSecretSlot) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || {
        let root = crate::paths::medousa_data_dir();
        delete_daemon_secret(&root, &secret_path(&id, slot)?)
    })
    .await?
}

fn normalize_instinct_phone(raw: &str) -> Result<String, HttpError> {
    let raw = raw.trim();
    if !raw.starts_with('+')
        || raw[1..]
            .chars()
            .any(|c| !c.is_ascii_digit() && !matches!(c, ' ' | '-' | '(' | ')'))
    {
        return Err(bad_request(
            "use an international WhatsApp phone number starting with +",
        ));
    }
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if !(7..=15).contains(&digits.len()) || digits.starts_with('0') {
        return Err(bad_request("invalid international WhatsApp phone number"));
    }
    Ok(format!("{digits}@s.whatsapp.net"))
}

fn valid_slack_id(value: &str, prefixes: &[u8]) -> bool {
    value.len() >= 9
        && value.len() <= 32
        && prefixes.contains(&value.as_bytes()[0])
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn sender_matches_dot(dot_id: Option<&str>, sender_id: &str, bot_id: Option<&str>) -> bool {
    dot_id.is_some_and(|dot_id| dot_id == sender_id || Some(dot_id) == bot_id)
}

pub async fn create(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(mut input): Json<CreateConversationRequest>,
) -> Result<Json<CreateConversationResponse>, HttpError> {
    let owner_id = owner(&principal, &state);
    if input.label.trim().is_empty()
        || input.label.len() > 120
        || input.target.trim().is_empty()
        || input.target.len() > 256
    {
        return Err(bad_request("invalid label or target"));
    }
    if input.provider != Provider::Dots
        && (input.slack_user_token.is_some() || input.dot_user_id.is_some())
    {
        return Err(bad_request("Slack settings are only for Dots"));
    }
    match input.provider {
        Provider::Muse => {
            if input.webhook_url.is_some() || input.webhook_key.is_some() {
                return Err(bad_request("Muse does not use webhook settings"));
            }
            let discovered = state
                .external_conversations
                .muse_discovery(&owner_id)
                .await
                .and_then(|status| status.observed_chat_jid);
            if discovered.as_deref() != Some(input.target.trim()) {
                return Err(bad_request(
                    "Discover the Muse chat through WhatsApp before connecting",
                ));
            }
        }
        Provider::Instinct => {
            if input.webhook_url.is_some() || input.webhook_key.is_some() {
                return Err(bad_request(
                    "Instinct uses a WhatsApp phone number, not a webhook",
                ));
            }
            input.target = normalize_instinct_phone(&input.target)?;
        }
        Provider::Dots => {
            if input.webhook_url.is_some() || input.webhook_key.is_some() {
                return Err(bad_request("Dots uses Slack, not a webhook"));
            }
            let channel = input.target.trim();
            let dot = input.dot_user_id.as_deref().unwrap_or("").trim();
            let token = input.slack_user_token.as_deref().unwrap_or("").trim();
            if !valid_slack_id(channel, b"CG") || !valid_slack_id(dot, b"UWB") {
                return Err(bad_request("Enter a Slack channel ID and dot member ID"));
            }
            if !token.starts_with("xoxp-") || token.len() < 20 {
                return Err(bad_request(
                    "Enter a Slack user token with chat:write access",
                ));
            }
            input.target = channel.to_string();
            input.dot_user_id = Some(dot.to_string());
        }
        Provider::GrokBot => {
            validate_webhook_url(
                input
                    .webhook_url
                    .as_deref()
                    .ok_or_else(|| bad_request("webhook URL required"))?,
            )?;
            if input
                .webhook_key
                .as_deref()
                .is_none_or(|key| key.trim().len() < 16)
            {
                return Err(bad_request("webhook sender key required"));
            }
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let callback_key =
        (input.provider == Provider::GrokBot).then(|| uuid::Uuid::new_v4().to_string());
    if let Some(key) = input.webhook_key.clone() {
        save_secret(id.clone(), IntegrationSecretSlot::AuthKey, key)
            .await
            .map_err(internal)?;
    }
    if let Some(token) = input.slack_user_token.clone() {
        save_secret(id.clone(), IntegrationSecretSlot::BotToken, token)
            .await
            .map_err(internal)?;
    }
    if let Some(key) = callback_key.as_ref()
        && let Err(error) =
            save_secret(id.clone(), IntegrationSecretSlot::AppToken, key.clone()).await
    {
        let _ = delete_secret(id.clone(), IntegrationSecretSlot::AuthKey).await;
        let _ = delete_secret(id.clone(), IntegrationSecretSlot::BotToken).await;
        return Err(internal(error));
    }
    let created = state
        .external_conversations
        .create(id.clone(), owner_id.clone(), &input)
        .await;
    let conversation = match created {
        Ok(conversation) => conversation,
        Err(error) => {
            let _ = delete_secret(id.clone(), IntegrationSecretSlot::AuthKey).await;
            let _ = delete_secret(id.clone(), IntegrationSecretSlot::BotToken).await;
            let _ = delete_secret(id, IntegrationSecretSlot::AppToken).await;
            return Err(internal(error));
        }
    };
    if input.provider == Provider::Muse {
        state
            .external_conversations
            .clear_muse_discovery(&owner_id)
            .await;
    }
    Ok(Json(CreateConversationResponse {
        conversation,
        callback_key,
    }))
}

pub async fn start_muse_discovery(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Json<MuseDiscoveryStatus> {
    Json(
        state
            .external_conversations
            .start_muse_discovery(owner(&principal, &state))
            .await,
    )
}

pub async fn get_muse_discovery(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Result<Json<MuseDiscoveryStatus>, HttpError> {
    state
        .external_conversations
        .muse_discovery(&owner(&principal, &state))
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "Muse discovery expired".into()))
}

pub async fn list(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> Json<ConversationListResponse> {
    Json(ConversationListResponse {
        conversations: state
            .external_conversations
            .list(&owner(&principal, &state))
            .await,
    })
}

pub async fn get_one(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<String>,
) -> Result<Json<ConversationView>, HttpError> {
    state
        .external_conversations
        .get(&owner(&principal, &state), &id)
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "conversation not found".into()))
}

pub async fn rotate_callback_key(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<String>,
) -> Result<Json<RotateExternalCallbackResponse>, HttpError> {
    let _send_guard = state.external_conversations.send_lock(&id).await;
    let binding = state
        .external_conversations
        .binding(&id)
        .await
        .filter(|record| {
            record.owner_id == owner(&principal, &state) && record.provider == Provider::GrokBot
        })
        .ok_or((
            StatusCode::NOT_FOUND,
            "Grok Bot conversation not found".into(),
        ))?;
    let callback_key = uuid::Uuid::new_v4().to_string();
    save_secret(
        binding.id,
        IntegrationSecretSlot::AppToken,
        callback_key.clone(),
    )
    .await
    .map_err(internal)?;
    Ok(Json(RotateExternalCallbackResponse { callback_key }))
}

pub async fn remove(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<String>,
) -> Result<Json<DeleteExternalConversationResponse>, HttpError> {
    let _send_guard = state.external_conversations.send_lock(&id).await;
    let deleted = state
        .external_conversations
        .remove(&owner(&principal, &state), &id)
        .await
        .map_err(internal)?;
    if !deleted {
        return Err((StatusCode::NOT_FOUND, "conversation not found".into()));
    }
    delete_secret(id.clone(), IntegrationSecretSlot::AuthKey)
        .await
        .map_err(internal)?;
    delete_secret(id.clone(), IntegrationSecretSlot::BotToken)
        .await
        .map_err(internal)?;
    delete_secret(id, IntegrationSecretSlot::AppToken)
        .await
        .map_err(internal)?;
    Ok(Json(DeleteExternalConversationResponse { deleted: true }))
}

pub async fn send(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<String>,
    Json(input): Json<SendMessageRequest>,
) -> Result<Json<ConversationView>, HttpError> {
    send_admitted(state, principal, id, input, false).await
}

fn validate_send_admission(
    principal: &RequestPrincipal,
    input: &SendMessageRequest,
    scheduled: bool,
) -> Result<(), HttpError> {
    if input.coordinator_wake && input.work.is_none() {
        return Err(bad_request("coordinator wake requires exact work metadata"));
    }
    if input.request_id.trim().is_empty()
        || input.request_id.len() > 128
        || input.text.trim().is_empty()
        || input.text.len() > 16 * 1024
        || (input.work.is_some() && input.text.len() > 12 * 1024)
    {
        return Err(bad_request("invalid request ID or message"));
    }
    if let Some(source) = &input.after_provider_completion
        && [&source.conversation_id, &source.request_id]
            .iter()
            .any(|id| id.trim().is_empty() || id.len() > 128 || id.chars().any(char::is_control))
    {
        return Err(bad_request("invalid provider predecessor identity"));
    }
    if input.after_native_completion && input.after_provider_completion.is_some() {
        return Err(bad_request(
            "native and provider completion triggers are mutually exclusive",
        ));
    }
    if (input.after_native_completion && scheduled)
        || (!scheduled
            && (input.after_native_completion
                || input.after_provider_completion.is_some()
                || input.coordinator_wake)
            && !principal.capabilities().contains(Capability::AdminExecute))
    {
        return Err((
            StatusCode::FORBIDDEN,
            "scheduled handoff or model wake requires native operator admission".into(),
        ));
    }
    Ok(())
}

async fn send_admitted(
    state: AppState,
    principal: RequestPrincipal,
    id: String,
    mut input: SendMessageRequest,
    scheduled: bool,
) -> Result<Json<ConversationView>, HttpError> {
    validate_send_admission(&principal, &input, scheduled)?;
    // Hold both immutable conversation identities in a fixed order through
    // dispatch admission. Removal/rotation cannot race the predecessor check.
    let mut lock_ids = vec![id.clone()];
    if let Some(source) = &input.after_provider_completion {
        lock_ids.push(source.conversation_id.clone());
    }
    lock_ids.sort();
    lock_ids.dedup();
    let mut _send_guards = vec![];
    for key in lock_ids {
        _send_guards.push(state.external_conversations.send_lock(&key).await);
    }
    let binding = state
        .external_conversations
        .binding(&id)
        .await
        .filter(|record| record.owner_id == owner(&principal, &state))
        .ok_or((StatusCode::NOT_FOUND, "conversation not found".into()))?;
    if input.after_native_completion || (input.after_provider_completion.is_some() && !scheduled) {
        return work_dispatch::admit(&state, &binding, input).await;
    }
    if let Some(host) = crate::daemon::work_units::local_work_unit_host() {
        let domain = provider_work_domain(&binding.owner_id)?;
        if let Some(saved) = host
            .pending_provider_dispatch(domain.clone(), id.clone(), input.request_id.clone())
            .await
            .map_err(internal)?
        {
            if scheduled
                && (saved.dispatch.target_digest
                    != work_dispatch::target_digest(&binding).map_err(internal)?
                    || !work_dispatch::source_matches(&state, &saved.dispatch).await)
            {
                host.close_provider_dispatch(
                    domain,
                    saved.dispatch,
                    "provider handoff source or destination changed before dispatch".into(),
                )
                .await
                .map_err(internal)?;
                return Err((
                    StatusCode::CONFLICT,
                    "saved provider handoff destination changed".into(),
                ));
            }
            if !scheduled
                || saved.dispatch.target_digest
                    != work_dispatch::target_digest(&binding).map_err(internal)?
                || input.work.as_ref() != Some(&saved.dispatch.input)
                || input.text != saved.dispatch.instructions
                || input.coordinator_wake != saved.dispatch.coordinator_wake
                || input.after_provider_completion.as_ref()
                    != saved
                        .dispatch
                        .after_provider_completion
                        .as_ref()
                        .map(|source| &source.request)
                || !host
                    .provider_dispatch_ready(domain, saved.dispatch)
                    .await
                    .map_err(internal)?
            {
                return Err((
                    StatusCode::CONFLICT,
                    "provider handoff is not admitted for this send; inspect saved dispatch".into(),
                ));
            }
        } else if scheduled {
            return Err((
                StatusCode::CONFLICT,
                "saved provider handoff admission missing".into(),
            ));
        }
    } else if scheduled {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "work unit host unavailable".into(),
        ));
    }
    if binding.events.iter().any(|event| {
        event.request_id.as_deref() == Some(&input.request_id)
            && event.kind == EventKind::UserMessage
    }) {
        return Err((
            StatusCode::CONFLICT,
            "request ID already used; inspect conversation before retry".into(),
        ));
    }
    let grok_settings = if binding.provider == Provider::GrokBot {
        let url = binding.webhook_url.clone().ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "Grok Bot webhook URL missing".into(),
        ))?;
        let key = load_secret(id.clone(), IntegrationSecretSlot::AuthKey)
            .await
            .map_err(internal)?
            .ok_or((
                StatusCode::SERVICE_UNAVAILABLE,
                "Grok Bot webhook key missing".into(),
            ))?;
        Some((url, key))
    } else {
        None
    };
    let dots_token = if binding.provider == Provider::Dots {
        Some(
            load_secret(id.clone(), IntegrationSecretSlot::BotToken)
                .await
                .map_err(internal)?
                .ok_or((
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Dots Slack user token missing".into(),
                ))?,
        )
    } else {
        None
    };
    let work = if let Some(work) = input.work.clone() {
        let host = crate::daemon::work_units::local_work_unit_host().ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "work unit host unavailable".into(),
        ))?;
        let domain = provider_work_domain(&binding.owner_id)?;
        let request = host
            .prepare_provider_request(
                domain.clone(),
                id.clone(),
                binding.provider,
                input.request_id.clone(),
                work,
                input.text.clone(),
            )
            .await
            .map_err(internal)?;
        if input.coordinator_wake {
            crate::daemon::work_units::model_wake::admit(&state, domain.clone(), &request)
                .await
                .map_err(internal)?;
        }
        input.text.push_str(&format!("\n\nmedousa-work-provider-v1: {}\nReport progress/completed/failed with your Work credential to Medousa's work-events endpoint, naming this exact request. Ordinary chat replies are not completion receipts.", serde_json::to_string(&request).map_err(internal)?));
        if request.reviewed.is_some() {
            input.text.push_str("\nmedousa-work-review-v1: inspect the pinned clean checkout without editing it. Completed result must be strict JSON with the exact reviewed object above, verdict approved or changes_requested, and a nonempty summary.");
        }
        if let Some(pin) = &request.predecessor {
            let budget = (16usize * 1024)
                .saturating_sub(input.text.len() + 64)
                .min(8192);
            let context = host
                .provider_predecessor_context(domain.clone(), pin.clone(), budget)
                .await
                .map_err(internal)?;
            input
                .text
                .push_str(&format!("\n\nmedousa-work-provider-source-v1: {context}"));
        }
        if input.text.len() > 16 * 1024 {
            return Err(bad_request(
                "work instructions and derived context exceed message bound",
            ));
        }
        Some((host, domain, request))
    } else {
        None
    };
    state
        .external_conversations
        .record(
            &id,
            format!("user:{}", input.request_id),
            Some(input.request_id.clone()),
            EventKind::UserMessage,
            input.text.clone(),
        )
        .await
        .map_err(internal)?;
    state
        .external_conversations
        .record(
            &id,
            format!("attempt:{}", input.request_id),
            Some(input.request_id.clone()),
            EventKind::TransportPending,
            "Sending to provider".into(),
        )
        .await
        .map_err(internal)?;
    if let Some((host, domain, request)) = work {
        host.claim_provider_request(domain, request)
            .await
            .map_err(internal)?;
    }
    let outcome: Result<(), (EventKind, String)> = match binding.provider {
        Provider::Muse | Provider::Instinct => {
            let target = crate::turn_scope::ChannelDeliveryTarget::interactive(
                "whatsapp",
                binding.owner_id.clone(),
                format!("whatsapp:chat:{}", binding.target),
                binding.id.clone(),
                input.request_id.clone(),
            );
            crate::channel_delivery::dispatch_channel_message(
                &state.channel_dispatch_client,
                &target,
                &input.text,
            )
            .await
            .map_err(|_| {
                (
                    EventKind::TransportUncertain,
                    "WhatsApp adapter could not confirm delivery; check the native chat".into(),
                )
            })
        }
        Provider::Dots => {
            let token = dots_token.expect("Dots token checked before journal commit");
            let dot = binding.dot_user_id.as_deref().unwrap_or_default();
            let response = state
                .channel_dispatch_client
                .post("https://slack.com/api/chat.postMessage")
                .bearer_auth(token)
                .json(&serde_json::json!({
                    "channel": binding.target,
                    "text": format!("<@{dot}> {}", input.text),
                    "unfurl_links": false,
                    "unfurl_media": false,
                }))
                .send()
                .await;
            match response {
                Ok(response) => match response.json::<serde_json::Value>().await {
                    Ok(body) if body.get("ok").and_then(|value| value.as_bool()) == Some(true) => {
                        Ok(())
                    }
                    Ok(body) => Err((
                        EventKind::TransportFailed,
                        format!(
                            "Slack rejected the message: {}",
                            body.get("error")
                                .and_then(|value| value.as_str())
                                .unwrap_or("unknown")
                        ),
                    )),
                    Err(_) => Err((
                        EventKind::TransportUncertain,
                        "Slack response could not be read; check the channel".into(),
                    )),
                },
                Err(_) => Err((
                    EventKind::TransportUncertain,
                    "Slack delivery is uncertain; check the channel".into(),
                )),
            }
        }
        Provider::GrokBot => {
            let (url, key) =
                grok_settings.expect("Grok Bot settings checked before journal commit");
            let client = reqwest::Client::builder()
                .redirect(Policy::none())
                .timeout(Duration::from_secs(15))
                .build()
                .map_err(internal)?;
            let response = client
                .post(url)
                .bearer_auth(key)
                .json(&serde_json::json!({
                    "schema_version": 1, "request_id": input.request_id,
                    "conversation_id": id, "message": input.text,
                }))
                .send()
                .await;
            match response {
                Ok(response) if response.status() == reqwest::StatusCode::OK => Ok(()),
                Ok(response) => Err((
                    EventKind::TransportFailed,
                    format!("Grok Bot webhook returned {}", response.status()),
                )),
                Err(_) => Err((
                    EventKind::TransportUncertain,
                    "Grok Bot webhook outcome is unknown; check routine history".into(),
                )),
            }
        }
    };
    let (kind, text) = match outcome {
        Ok(()) => (
            EventKind::TransportAccepted,
            "Transport accepted the message".to_string(),
        ),
        Err(failure) => failure,
    };
    let view = state
        .external_conversations
        .record(
            &id,
            format!("transport:{}", input.request_id),
            Some(input.request_id),
            kind,
            text,
        )
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "conversation not found".into()))?;
    Ok(Json(view))
}

pub async fn provider_event(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<ProviderEventRequest>,
) -> Result<Json<ConversationView>, HttpError> {
    let binding = state
        .external_conversations
        .binding(&id)
        .await
        .filter(|record| record.owner_id == owner(&principal, &state))
        .ok_or((StatusCode::NOT_FOUND, "conversation not found".into()))?;
    if binding.provider != Provider::GrokBot
        || input.kind == EventKind::UserMessage
        || input.kind == EventKind::TransportPending
        || input.kind == EventKind::TransportAccepted
        || input.kind == EventKind::TransportFailed
        || input.kind == EventKind::TransportUncertain
        || input.event_id.trim().is_empty()
        || input.event_id.len() > 128
        || input.text.len() > 16 * 1024
        || input
            .reaction
            .as_ref()
            .is_some_and(|reaction| reaction.emoji.trim().is_empty())
    {
        return Err(bad_request("invalid provider event"));
    }
    let supplied = headers
        .get("x-medousa-bridge-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let expected = load_secret(id.clone(), IntegrationSecretSlot::AppToken)
        .await
        .map_err(internal)?
        .ok_or((
            StatusCode::SERVICE_UNAVAILABLE,
            "callback credential missing".into(),
        ))?;
    if supplied.is_empty() || !constant_time_equal(supplied.as_bytes(), expected.as_bytes()) {
        return Err((StatusCode::FORBIDDEN, "invalid callback credential".into()));
    }
    if input.request_id.trim().is_empty()
        || !binding.events.iter().any(|event| {
            event.kind == EventKind::UserMessage
                && event.request_id.as_deref() == Some(&input.request_id)
        })
    {
        return Err((StatusCode::CONFLICT, "unknown request ID".into()));
    }
    let event_id = format!("provider:{}", input.event_id);
    if let Some(host) = crate::daemon::work_units::local_work_unit_host() {
        let domain = provider_work_domain(&binding.owner_id)?;
        if host
            .provider_request(domain.clone(), id.clone(), input.request_id.clone())
            .await
            .map_err(internal)?
            .is_some()
            && matches!(
                input.kind,
                EventKind::Progress
                    | EventKind::Question
                    | EventKind::Completed
                    | EventKind::Failed
            )
            && input.reaction.is_none()
        {
            host.record_provider_outcome(
                domain,
                id.clone(),
                format!("provider-bridge:{id}"),
                input.clone(),
            )
            .await
            .map_err(internal)?;
        }
    }
    let view = if let Some(reaction) = input.reaction {
        state
            .external_conversations
            .record_reaction(
                &id,
                event_id,
                Some(input.request_id),
                input.kind,
                input.text,
                reaction,
            )
            .await
    } else {
        state
            .external_conversations
            .record(
                &id,
                event_id,
                Some(input.request_id),
                input.kind,
                input.text,
            )
            .await
    };
    view.map_err(internal)?
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "conversation not found".into()))
}

fn provider_work_domain(owner: &str) -> Result<medousa_types::work_unit::UserDomainRef, HttpError> {
    Ok(medousa_types::work_unit::UserDomainRef {
        authority_id: crate::workshop_authority::current()
            .map_err(internal)?
            .clone(),
        user_id: owner.into(),
    })
}

/// Narrow provider-owned callback; the bearer credential must belong to this
/// exact conversation. No admin capability or transport identity can substitute.
pub async fn work_event(
    State(state): State<AppState>,
    Extension(principal): Extension<RequestPrincipal>,
    Path(id): Path<String>,
    Json(input): Json<ProviderEventRequest>,
) -> Result<Json<ConversationView>, HttpError> {
    if !state
        .external_conversations
        .permits_work_callback(&principal, &id)
        .await
    {
        return Err((
            StatusCode::FORBIDDEN,
            "work callback requires this provider's current Work credential".into(),
        ));
    }
    let host = crate::daemon::work_units::local_work_unit_host().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "work unit host unavailable".into(),
    ))?;
    record_work_event(&state.external_conversations, &host, &principal, &id, input)
        .await
        .map(Json)
}

async fn record_work_event(
    store: &ExternalConversationStore,
    host: &crate::daemon::work_units::WorkUnitHost,
    principal: &RequestPrincipal,
    id: &str,
    input: ProviderEventRequest,
) -> Result<ConversationView, HttpError> {
    if !store.permits_work_callback(principal, id).await {
        return Err((
            StatusCode::FORBIDDEN,
            "work callback requires this provider's current Work credential".into(),
        ));
    }
    if input.event_id.trim().is_empty()
        || input.event_id.len() > 128
        || input.request_id.trim().is_empty()
        || input.request_id.len() > 128
        || input.text.len() > 16 * 1024
        || input.reaction.is_some()
        || !matches!(
            input.kind,
            EventKind::Progress | EventKind::Question | EventKind::Completed | EventKind::Failed
        )
    {
        return Err(bad_request("invalid provider work event"));
    }
    let domain = provider_work_domain(principal.profile_id().expect("callback owner checked"))?;
    let copy = input.clone();
    host.record_provider_outcome(
        domain,
        id.to_owned(),
        principal
            .credential_id()
            .expect("callback credential checked")
            .as_str()
            .into(),
        copy,
    )
    .await
    .map_err(|error| (StatusCode::CONFLICT, error.to_string()))?;
    // The durable work receipt precedes this conversation mirror. A mirror
    // failure can retry the same callback without losing or replacing evidence.
    let view = store
        .record(
            id,
            format!("provider:{}", input.event_id),
            Some(input.request_id),
            input.kind,
            input.text,
        )
        .await
        .map_err(internal)?
        .ok_or((StatusCode::NOT_FOUND, "conversation not found".into()))?;
    Ok(view)
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut diff = left.len() ^ right.len();
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= usize::from(a ^ b);
    }
    diff == 0
}

pub async fn whatsapp_inbound(
    State(state): State<AppState>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
    Json(input): Json<WhatsAppInboundRequest>,
) -> Result<Json<InboundClaimResponse>, HttpError> {
    if !source.ip().is_loopback() {
        return Err((
            StatusCode::FORBIDDEN,
            "WhatsApp adapter must connect locally".into(),
        ));
    }
    if input.chat_jid.len() > 256
        || input.sender_jid.len() > 256
        || input.message_id.trim().is_empty()
        || input.message_id.len() > 128
        || input.text.len() > 16 * 1024
        || input
            .reaction
            .as_ref()
            .is_some_and(|reaction| reaction.emoji.trim().is_empty())
    {
        return Err(bad_request("invalid WhatsApp message"));
    }
    if state
        .external_conversations
        .observe_muse_challenge(&input.chat_jid, &input.text)
        .await
    {
        return Ok(Json(InboundClaimResponse { claimed: true }));
    }
    let Some(binding) = state
        .external_conversations
        .whatsapp_binding(&input.chat_jid)
        .await
    else {
        return Ok(Json(InboundClaimResponse { claimed: false }));
    };
    let event_id = format!("whatsapp:{}", input.message_id);
    if let Some(reaction) = input.reaction {
        state
            .external_conversations
            .record_reaction(
                &binding.id,
                event_id,
                None,
                EventKind::ProviderMessage,
                input.text,
                reaction,
            )
            .await
    } else {
        state
            .external_conversations
            .record(
                &binding.id,
                event_id,
                None,
                EventKind::ProviderMessage,
                input.text,
            )
            .await
    }
    .map_err(internal)?;
    Ok(Json(InboundClaimResponse { claimed: true }))
}

pub async fn slack_inbound(
    State(state): State<AppState>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
    Json(input): Json<SlackInboundRequest>,
) -> Result<Json<InboundClaimResponse>, HttpError> {
    if !source.ip().is_loopback() {
        return Err((
            StatusCode::FORBIDDEN,
            "Slack adapter must connect locally".into(),
        ));
    }
    if !valid_slack_id(&input.channel_id, b"CG")
        || !valid_slack_id(&input.sender_id, b"UWB")
        || input
            .bot_id
            .as_deref()
            .is_some_and(|bot_id| !valid_slack_id(bot_id, b"B"))
        || input.message_id.is_empty()
        || input.message_id.len() > 128
        || input.text.len() > 16 * 1024
    {
        return Err(bad_request("invalid Slack message"));
    }
    let Some(binding) = state
        .external_conversations
        .dots_binding(&input.channel_id)
        .await
    else {
        return Ok(Json(InboundClaimResponse { claimed: false }));
    };
    if !sender_matches_dot(
        binding.dot_user_id.as_deref(),
        &input.sender_id,
        input.bot_id.as_deref(),
    ) {
        return Ok(Json(InboundClaimResponse { claimed: true }));
    }
    state
        .external_conversations
        .record(
            &binding.id,
            format!("slack:{}:{}", input.channel_id, input.message_id),
            None,
            EventKind::ProviderMessage,
            input.text,
        )
        .await
        .map_err(internal)?;
    Ok(Json(InboundClaimResponse { claimed: true }))
}

pub async fn get_whatsapp_pairing(
    State(state): State<AppState>,
) -> Result<Json<PairingStatus>, HttpError> {
    Ok(Json(state.external_conversations.whatsapp_pairing().await))
}

pub async fn update_whatsapp_pairing(
    State(state): State<AppState>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
    Json(update): Json<PairingUpdateRequest>,
) -> Result<Json<PairingStatus>, HttpError> {
    if !source.ip().is_loopback() {
        return Err((
            StatusCode::FORBIDDEN,
            "WhatsApp adapter must connect locally".into(),
        ));
    }
    Ok(Json(
        state
            .external_conversations
            .update_whatsapp_pairing(update)
            .await
            .map_err(|message| bad_request(&message))?,
    ))
}

fn policy(method: Method, path: &'static str, capability: Capability, limit: usize) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Portal,
        required_capability: Some(capability),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit: limit,
        rate_limit_class: RateLimitClass::Mutation,
    }
}

fn whatsapp_pairing_read_policy() -> RoutePolicy {
    RoutePolicy {
        rate_limit_class: RateLimitClass::Read,
        ..policy(
            Method::GET,
            "/v1/external-conversations/whatsapp/pairing",
            Capability::AdminExecute,
            1024,
        )
    }
}

pub fn surface() -> DeclaredRouter<AppState> {
    DeclaredRouter::default()
        .route(
            policy(
                Method::POST,
                "/v1/external-conversations/{id}/work-events",
                Capability::ContentWrite,
                20 * 1024,
            ),
            post(work_event),
        )
        .methods([
            (
                policy(
                    Method::GET,
                    "/v1/external-conversations",
                    Capability::WorkshopRead,
                    1024,
                ),
                get(list),
            ),
            (
                policy(
                    Method::POST,
                    "/v1/external-conversations",
                    Capability::AdminExecute,
                    20 * 1024,
                ),
                post(create),
            ),
        ])
        .methods([
            (
                policy(
                    Method::GET,
                    "/v1/external-conversations/muse/discovery",
                    Capability::WorkshopRead,
                    1024,
                ),
                get(get_muse_discovery),
            ),
            (
                policy(
                    Method::POST,
                    "/v1/external-conversations/muse/discovery",
                    Capability::AdminExecute,
                    1024,
                ),
                post(start_muse_discovery),
            ),
        ])
        .methods([
            (whatsapp_pairing_read_policy(), get(get_whatsapp_pairing)),
            (
                policy(
                    Method::POST,
                    "/v1/external-conversations/whatsapp/pairing",
                    Capability::WorkshopInteract,
                    4096,
                ),
                post(update_whatsapp_pairing),
            ),
        ])
        .methods([
            (
                policy(
                    Method::GET,
                    "/v1/external-conversations/{id}",
                    Capability::WorkshopRead,
                    1024,
                ),
                get(get_one),
            ),
            (
                policy(
                    Method::DELETE,
                    "/v1/external-conversations/{id}",
                    Capability::AdminExecute,
                    1024,
                ),
                delete(remove),
            ),
        ])
        .methods([
            (
                policy(
                    Method::POST,
                    "/v1/external-conversations/{id}/api-token",
                    Capability::AdminExecute,
                    4096,
                ),
                post(access::issue),
            ),
            (
                policy(
                    Method::DELETE,
                    "/v1/external-conversations/{id}/api-token",
                    Capability::AdminExecute,
                    1024,
                ),
                delete(access::revoke),
            ),
        ])
        .route(
            policy(
                Method::POST,
                "/v1/external-conversations/{id}/callback-key/rotate",
                Capability::AdminExecute,
                1024,
            ),
            post(rotate_callback_key),
        )
        .route(
            policy(
                Method::POST,
                "/v1/external-conversations/{id}/messages",
                Capability::WorkshopInteract,
                20 * 1024,
            ),
            post(send),
        )
        .route(
            policy(
                Method::POST,
                "/v1/external-conversations/{id}/events",
                Capability::WorkshopInteract,
                20 * 1024,
            ),
            post(provider_event),
        )
        .route(
            policy(
                Method::POST,
                "/v1/external-conversations/whatsapp/inbound",
                Capability::WorkshopInteract,
                20 * 1024,
            ),
            post(whatsapp_inbound),
        )
        .route(
            policy(
                Method::POST,
                "/v1/external-conversations/slack/inbound",
                Capability::WorkshopInteract,
                20 * 1024,
            ),
            post(slack_inbound),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_chain_intent_cannot_mint_operator_authority_from_work_or_worker_credentials() {
        let mut input: SendMessageRequest = serde_json::from_value(serde_json::json!({
            "request_id": "next", "text": "Analyze the predecessor",
            "after_provider_completion": {"conversation_id":"source", "request_id":"first"},
            "work": {"work_unit_id":"unit", "expected_scope_revision":1, "deadline":chrono::Utc::now()+chrono::Duration::hours(1)}
        })).unwrap();
        let external = RequestPrincipal::external_agent(
            Arc::from("work-token"),
            "owner".into(),
            true,
            crate::request_principal::TransportClass::Loopback,
        );
        for principal in [
            external,
            RequestPrincipal::worker("owner"),
            RequestPrincipal::continuation("owner"),
        ] {
            assert_eq!(
                validate_send_admission(&principal, &input, false)
                    .unwrap_err()
                    .0,
                StatusCode::FORBIDDEN
            );
        }
        let operator = RequestPrincipal::local_app(
            Arc::from("local-app"),
            crate::request_principal::TransportClass::Loopback,
        );
        validate_send_admission(&operator, &input, false).unwrap();
        input.after_native_completion = true;
        assert_eq!(
            validate_send_admission(&operator, &input, false)
                .unwrap_err()
                .0,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn journal_replays_and_deduplicates() {
        let path =
            std::env::temp_dir().join(format!("medousa-external-{}.json", uuid::Uuid::new_v4()));
        let store = ExternalConversationStore::open(path.clone()).await.unwrap();
        let input = CreateConversationRequest {
            provider: Provider::Muse,
            label: "Muse".into(),
            target: "123@s.whatsapp.net".into(),
            webhook_url: None,
            webhook_key: None,
            slack_user_token: None,
            dot_user_id: None,
        };
        let created = store
            .create(uuid::Uuid::new_v4().to_string(), "owner".into(), &input)
            .await
            .unwrap();
        assert!(store.whatsapp_binding("999@s.whatsapp.net").await.is_none());
        assert_eq!(
            store
                .whatsapp_binding("123@s.whatsapp.net")
                .await
                .unwrap()
                .id,
            created.id
        );
        assert!(
            store
                .create(uuid::Uuid::new_v4().to_string(), "other".into(), &input)
                .await
                .is_err()
        );
        store
            .record(
                &created.id,
                "wa:1".into(),
                None,
                EventKind::ProviderMessage,
                "hello".into(),
            )
            .await
            .unwrap();
        store
            .record(
                &created.id,
                "wa:1".into(),
                None,
                EventKind::ProviderMessage,
                "hello".into(),
            )
            .await
            .unwrap();
        drop(store);
        let reopened = ExternalConversationStore::open(path.clone()).await.unwrap();
        assert_eq!(
            reopened
                .get("owner", &created.id)
                .await
                .unwrap()
                .events
                .len(),
            1
        );
        assert!(reopened.get("other", &created.id).await.is_none());
        assert!(!reopened.remove("other", &created.id).await.unwrap());
        assert!(reopened.remove("owner", &created.id).await.unwrap());
        assert!(reopened.get("owner", &created.id).await.is_none());
        let _ = tokio::fs::remove_file(path).await;
    }

    #[tokio::test]
    async fn dots_binding_reserves_only_its_channel() {
        let path = std::env::temp_dir().join(format!("medousa-dots-{}.json", uuid::Uuid::new_v4()));
        let store = ExternalConversationStore::open(path.clone()).await.unwrap();
        let input = CreateConversationRequest {
            provider: Provider::Dots,
            label: "Dot".into(),
            target: "C123456789".into(),
            webhook_url: None,
            webhook_key: None,
            slack_user_token: None,
            dot_user_id: Some("U123456789".into()),
        };
        let created = store
            .create(uuid::Uuid::new_v4().to_string(), "owner".into(), &input)
            .await
            .unwrap();
        assert!(store.dots_binding("C999999999").await.is_none());
        assert_eq!(
            store.dots_binding("C123456789").await.unwrap().id,
            created.id
        );
        assert!(
            store
                .create(uuid::Uuid::new_v4().to_string(), "other".into(), &input)
                .await
                .is_err()
        );
        let _ = tokio::fs::remove_file(path).await;
    }

    #[test]
    fn dots_reply_matches_slack_member_or_bot_identity() {
        assert!(sender_matches_dot(Some("U123456789"), "U123456789", None));
        assert!(sender_matches_dot(
            Some("B123456789"),
            "U999999999",
            Some("B123456789")
        ));
        assert!(!sender_matches_dot(
            Some("B123456789"),
            "U999999999",
            Some("B999999999")
        ));
    }

    #[test]
    fn webhook_url_rejects_non_provider_and_non_https_targets() {
        assert!(validate_webhook_url("https://api.cursor.com/routine/1").is_ok());
        assert!(validate_webhook_url("https://api.cursor.sh/routine/1").is_ok());
        assert!(validate_webhook_url("http://api.cursor.com/routine/1").is_err());
        assert!(validate_webhook_url("https://api.cursor.com.evil.test/routine/1").is_err());
        assert!(validate_webhook_url("https://127.0.0.1/routine/1").is_err());
    }

    #[tokio::test]
    async fn muse_discovery_requires_challenge_in_an_individual_chat() {
        let path =
            std::env::temp_dir().join(format!("medousa-external-{}.json", uuid::Uuid::new_v4()));
        let store = ExternalConversationStore::open(path).await.unwrap();
        let started = store.start_muse_discovery("owner".into()).await;
        assert!(started.observed_chat_jid.is_none());
        assert!(!store.observe_muse_challenge("opaque@lid", "wrong").await);
        assert!(
            !store
                .observe_muse_challenge("group@g.us", &started.challenge)
                .await
        );
        assert!(
            store
                .observe_muse_challenge(
                    "opaque@lid",
                    &format!("Here is the code: {}", started.challenge),
                )
                .await
        );
        assert_eq!(
            store
                .muse_discovery("owner")
                .await
                .unwrap()
                .observed_chat_jid
                .as_deref(),
            Some("opaque@lid")
        );
        assert!(store.muse_discovery("other").await.is_none());
        store.clear_muse_discovery("owner").await;
        assert!(store.muse_discovery("owner").await.is_none());
    }

    #[tokio::test]
    async fn whatsapp_pairing_qr_expires_without_leaking_its_payload() {
        let path =
            std::env::temp_dir().join(format!("medousa-external-{}.json", uuid::Uuid::new_v4()));
        let store = ExternalConversationStore::open(path).await.unwrap();
        let status = store
            .update_whatsapp_pairing(PairingUpdateRequest {
                state: PairingState::QrReady,
                qr_code: Some("whatsapp-pairing-test".into()),
                expires_in_seconds: Some(1),
            })
            .await
            .unwrap();
        assert!(status.qr_svg.as_deref().unwrap().contains("<svg"));
        store.whatsapp_pairing.lock().await.expires_at = Some(Utc::now() - TimeDelta::seconds(1));
        let expired = store.whatsapp_pairing().await;
        assert_eq!(expired.state, PairingState::Waiting);
        assert!(expired.qr_svg.is_none());
        assert!(store.whatsapp_pairing.lock().await.qr_svg.is_none());
        let connected = store
            .update_whatsapp_pairing(PairingUpdateRequest {
                state: PairingState::Connected,
                qr_code: None,
                expires_in_seconds: None,
            })
            .await
            .unwrap();
        assert!(connected.qr_svg.is_none());
    }
}
