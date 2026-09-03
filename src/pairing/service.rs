use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use base64::Engine;
use chrono::{DateTime, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, OwnedSemaphorePermit, RwLock, Semaphore};
use uuid::Uuid;

use super::crypto::{
    PROTOCOL_VERSION, QR_SCHEME, QR_SCHEME_V2, base64url_decode, base64url_encode,
    hash_session_token, parse_verifying_key, qr_signing_message, qr_signing_message_v2,
    sign_message, verify_message, verifying_key_to_b64,
};
use super::identity::DeviceIdentity;
use super::store::{PairedDeviceRecord, PairingRole, PairingStore};
use crate::credential_lifecycle::{CredentialKind, CredentialLifecycle};

const QR_TTL: Duration = Duration::from_secs(300);
const VERIFY_TTL: Duration = Duration::from_secs(10);
const SESSION_REFRESH_TTL: Duration = Duration::from_secs(60);
const SESSION_TOKEN_TTL: Duration = Duration::from_secs(86_400);
const INIT_RATE_LIMIT: usize = 3;
const INIT_RATE_WINDOW: Duration = Duration::from_secs(60);
const GLOBAL_INIT_RATE_LIMIT: usize = 24;
const VERIFY_RATE_LIMIT: usize = 6;
const GLOBAL_VERIFY_RATE_LIMIT: usize = 48;
const REFRESH_RATE_LIMIT: usize = 12;
const GLOBAL_REFRESH_RATE_LIMIT: usize = 96;
const MAX_PENDING_SESSIONS: usize = 32;
const MAX_PENDING_REFRESH_SESSIONS: usize = 32;
const CEREMONY_CONCURRENCY: usize = 4;
const MIN_IDLE_TIMEOUT_SECONDS: u64 = 86_400;
const MAX_IDLE_TIMEOUT_SECONDS: u64 = 10 * 365 * 86_400;

const SHORT_CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrResponse {
    pub url: String,
    pub expires_at: DateTime<Utc>,
    pub short_code: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrImageResponse {
    pub url: String,
    pub expires_at: DateTime<Utc>,
    pub short_code: String,
    pub png_base64: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairStatusResponse {
    pub paired_devices: Vec<PairedDeviceSummary>,
    pub qr_active: bool,
    pub device_id: String,
    pub peer_name: String,
    pub protocol_version: String,
    pub daemon_public_key: String,
    pub iroh_available: bool,
    pub qr_protocol_version: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IrohTicketResponse {
    pub ticket: String,
    pub endpoint_id: String,
    pub available: bool,
}

/// Live workshop Iroh bootstrap material for QR v2.
#[derive(Debug, Clone)]
pub struct IrohWorkshopInfo {
    pub ticket: String,
    pub endpoint_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDeviceSummary {
    pub pairing_id: String,
    pub phone_id: String,
    pub phone_name: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub session_expires_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trust_expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_seconds: Option<u64>,
    pub trust_active: bool,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairInitRequest {
    pub qr_token: Option<String>,
    pub short_code: Option<String>,
    pub phone_id: String,
    pub phone_name: String,
    pub public_key: String,
    /// `portal` (full client) or `peer` (inbox/share only). Defaults to portal.
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairInitResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairVerifyRequest {
    pub session_id: String,
    pub signed_nonce: String,
    pub phone_nonce: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairVerifyResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_signed_nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pairing_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairSessionChallengeRequest {
    pub pairing_id: String,
    pub phone_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairSessionChallengeResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairSessionRefreshRequest {
    pub session_id: String,
    pub pairing_id: String,
    pub phone_id: String,
    pub signed_nonce: String,
    pub phone_nonce: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairSessionRefreshResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_signed_nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairTrustPolicyUpdateRequest {
    #[serde(default)]
    pub trust_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub idle_timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairHeartbeatResponse {
    pub status: String,
    pub device_time: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PairHeartbeatRequest {
    #[serde(default)]
    pub apns_device_token: Option<String>,
    #[serde(default)]
    pub push_platform: Option<String>,
    #[serde(default)]
    pub live_activity_push_token: Option<String>,
    /// Optional dial-back endpoint for mesh reverse delivery (M3 registry).
    #[serde(default)]
    pub mesh_lan_base_url: Option<String>,
    #[serde(default)]
    pub mesh_iroh_ticket: Option<String>,
    #[serde(default)]
    pub mesh_iroh_endpoint_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokePairingResult {
    Removed,
    NotFound,
    Unauthorized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevokePairingAuthority<'a> {
    Unauthenticated,
    Credential(&'a str),
    Administrator,
}

#[derive(Debug, Clone)]
struct ActiveQrSession {
    token_b64: String,
    short_code: String,
    short_code_raw: String,
    expires_at: DateTime<Utc>,
    used: bool,
    /// When set, successful verify binds the device to this Shared-mode profile.
    bound_profile_id: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingPairSession {
    phone_id: String,
    phone_name: String,
    phone_public_key: VerifyingKey,
    role: PairingRole,
    bound_profile_id: Option<String>,
    server_nonce: [u8; 32],
    created_at: Instant,
}

#[derive(Debug, Clone)]
struct PendingRefreshSession {
    pairing_id: String,
    phone_id: String,
    server_nonce: [u8; 32],
    created_at: Instant,
}

#[derive(Default)]
struct PairingAdmission {
    init_global: VecDeque<Instant>,
    init_by_source: HashMap<IpAddr, VecDeque<Instant>>,
    verify_global: VecDeque<Instant>,
    verify_by_source: HashMap<IpAddr, VecDeque<Instant>>,
    refresh_global: VecDeque<Instant>,
    refresh_by_source: HashMap<IpAddr, VecDeque<Instant>>,
}

impl PairingAdmission {
    fn allow_init(&mut self, source: IpAddr, now: Instant) -> bool {
        allow_attempt(
            &mut self.init_global,
            &mut self.init_by_source,
            source,
            now,
            INIT_RATE_WINDOW,
            GLOBAL_INIT_RATE_LIMIT,
            INIT_RATE_LIMIT,
        )
    }

    fn allow_verify(&mut self, source: IpAddr, now: Instant) -> bool {
        allow_attempt(
            &mut self.verify_global,
            &mut self.verify_by_source,
            source,
            now,
            INIT_RATE_WINDOW,
            GLOBAL_VERIFY_RATE_LIMIT,
            VERIFY_RATE_LIMIT,
        )
    }

    fn allow_refresh(&mut self, source: IpAddr, now: Instant) -> bool {
        allow_attempt(
            &mut self.refresh_global,
            &mut self.refresh_by_source,
            source,
            now,
            INIT_RATE_WINDOW,
            GLOBAL_REFRESH_RATE_LIMIT,
            REFRESH_RATE_LIMIT,
        )
    }
}

#[derive(Debug, Clone)]
pub struct ApnsPushTarget {
    pub phone_id: String,
    pub device_token: String,
}

#[derive(Debug, Clone)]
pub struct LiveActivityPushTarget {
    pub phone_id: String,
    pub push_token: String,
}

pub struct PairingService {
    identity: DeviceIdentity,
    store: PairingStore,
    advertise_address: String,
    peer_name: String,
    model_descriptor: Option<String>,
    auth_required: bool,
    iroh: Option<IrohWorkshopInfo>,
    active_qr: RwLock<Option<ActiveQrSession>>,
    pending_sessions: RwLock<HashMap<Uuid, PendingPairSession>>,
    pending_refresh_sessions: RwLock<HashMap<Uuid, PendingRefreshSession>>,
    admission: Mutex<PairingAdmission>,
    credential_rotation: Mutex<()>,
    ceremony_slots: Arc<Semaphore>,
    credential_lifecycle: CredentialLifecycle,
}

impl PairingService {
    pub fn new(
        identity: DeviceIdentity,
        advertise_address: String,
        peer_name: String,
        model_descriptor: Option<String>,
        iroh: Option<IrohWorkshopInfo>,
    ) -> Self {
        Self::new_with_lifecycle(
            identity,
            advertise_address,
            peer_name,
            model_descriptor,
            iroh,
            CredentialLifecycle::default(),
        )
    }

    pub fn new_with_lifecycle(
        identity: DeviceIdentity,
        advertise_address: String,
        peer_name: String,
        model_descriptor: Option<String>,
        iroh: Option<IrohWorkshopInfo>,
        credential_lifecycle: CredentialLifecycle,
    ) -> Self {
        let store = PairingStore::new(identity.signing_key());
        Self {
            identity,
            store,
            advertise_address,
            peer_name,
            model_descriptor,
            auth_required: true,
            iroh,
            active_qr: RwLock::new(None),
            pending_sessions: RwLock::new(HashMap::new()),
            pending_refresh_sessions: RwLock::new(HashMap::new()),
            admission: Mutex::new(PairingAdmission::default()),
            credential_rotation: Mutex::new(()),
            ceremony_slots: Arc::new(Semaphore::new(CEREMONY_CONCURRENCY)),
            credential_lifecycle,
        }
    }

    pub fn credential_lifecycle(&self) -> CredentialLifecycle {
        self.credential_lifecycle.clone()
    }

    pub fn iroh_ticket(&self) -> Option<IrohTicketResponse> {
        self.iroh.as_ref().map(|info| IrohTicketResponse {
            ticket: info.ticket.clone(),
            endpoint_id: info.endpoint_id.clone(),
            available: true,
        })
    }

    pub fn try_acquire_ceremony(&self) -> Option<OwnedSemaphorePermit> {
        self.ceremony_slots.clone().try_acquire_owned().ok()
    }

    pub fn iroh_available(&self) -> bool {
        self.iroh.is_some()
    }

    pub fn device_id(&self) -> &str {
        &self.identity.device_id
    }

    pub fn identity(&self) -> &DeviceIdentity {
        &self.identity
    }

    pub fn peer_name(&self) -> &str {
        &self.peer_name
    }

    pub fn advertise_address(&self) -> &str {
        &self.advertise_address
    }

    pub fn model_descriptor(&self) -> Option<&str> {
        self.model_descriptor.as_deref()
    }

    pub fn auth_required_flag(&self) -> &'static str {
        if self.auth_required { "1" } else { "0" }
    }

    pub fn capability_flags(&self) -> String {
        let mut flags: u16 = 0x001F;
        if self.iroh.is_some() {
            flags |= 0x0020;
        }
        format!("{flags:04X}")
    }

    pub fn mdns_service_type(&self) -> &'static str {
        "_medousa._tcp.local."
    }

    pub fn parse_advertise_port(&self) -> u16 {
        self.advertise_address
            .rsplit(':')
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(crate::daemon_api::DEFAULT_DAEMON_PORT)
    }

    pub fn list_paired_devices(&self) -> Result<Vec<PairedDeviceRecord>> {
        self.store.list_paired()
    }

    pub async fn pair_status(&self) -> Result<PairStatusResponse> {
        let paired = self.store.list_paired()?;
        let qr_active = self
            .active_qr
            .read()
            .await
            .as_ref()
            .is_some_and(|session| !session.used && session.expires_at > Utc::now());
        Ok(PairStatusResponse {
            paired_devices: paired
                .into_iter()
                .map(|record| paired_device_summary(record, Utc::now()))
                .collect(),
            qr_active,
            device_id: self.identity.device_id.clone(),
            peer_name: self.peer_name.clone(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            daemon_public_key: verifying_key_to_b64(self.identity.verifying_key()),
            iroh_available: self.iroh.is_some(),
            // Default QR/PNG is always compact v1 (camera-friendly). Full v2 via ?full=1.
            qr_protocol_version: super::crypto::QR_PROTOCOL_V1.to_string(),
        })
    }

    /// Default invite is compact v1 (camera / Messages friendly). Pass `full=true` for v2 with Iroh ticket.
    pub async fn current_qr(&self) -> Result<QrResponse> {
        self.current_qr_with_options(false).await
    }

    pub async fn current_qr_with_options(&self, full: bool) -> Result<QrResponse> {
        let mut guard = self.active_qr.write().await;
        let needs_refresh = guard
            .as_ref()
            .is_none_or(|session| session.used || session.expires_at <= Utc::now());
        if needs_refresh {
            *guard = Some(self.build_qr_session(None)?);
        }
        let session = guard.as_ref().expect("qr session");
        Ok(QrResponse {
            url: self.build_qr_url(session, full)?,
            expires_at: session.expires_at,
            short_code: session.short_code.clone(),
        })
    }

    /// Invalidate the current invite and mint a fresh QR (M4 invite rotation).
    pub async fn rotate_qr(&self) -> Result<QrResponse> {
        self.rotate_qr_for_profile(None).await
    }

    /// Mint a QR that binds the pairing device to `profile_id` (Shared-mode seat invite).
    pub async fn rotate_qr_for_profile(&self, profile_id: Option<&str>) -> Result<QrResponse> {
        let bound = profile_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let mut guard = self.active_qr.write().await;
        *guard = Some(self.build_qr_session(bound)?);
        let session = guard.as_ref().expect("qr session");
        Ok(QrResponse {
            url: self.build_qr_url(session, false)?,
            expires_at: session.expires_at,
            short_code: session.short_code.clone(),
        })
    }

    pub async fn current_short_code(&self) -> Result<String> {
        Ok(self.current_qr().await?.short_code)
    }

    pub async fn current_qr_image(&self) -> Result<QrImageResponse> {
        self.current_qr_image_with_options(false).await
    }

    pub async fn current_qr_image_with_options(&self, full: bool) -> Result<QrImageResponse> {
        let qr = self.current_qr_with_options(full).await?;
        let png = self.render_qr_png(&qr.url)?;
        Ok(QrImageResponse {
            url: qr.url,
            expires_at: qr.expires_at,
            short_code: qr.short_code,
            png_base64: base64::engine::general_purpose::STANDARD.encode(png),
        })
    }

    pub async fn pair_init(
        &self,
        request: PairInitRequest,
        source_ip: IpAddr,
    ) -> Result<PairInitResponse> {
        if !self
            .admission
            .lock()
            .await
            .allow_init(source_ip, Instant::now())
        {
            return Ok(rejected_init("rate_limited"));
        }

        let phone_key = parse_verifying_key(&request.public_key)?;
        let token = if let Some(code) = request.short_code.as_deref() {
            self.resolve_short_code(code).await?
        } else if let Some(token) = request.qr_token.as_deref() {
            token.to_string()
        } else {
            return Ok(rejected_init("invalid_invite"));
        };

        let mut pending_sessions = self.pending_sessions.write().await;
        pending_sessions.retain(|_, pending| pending.created_at.elapsed() <= VERIFY_TTL);
        if pending_sessions.len() >= MAX_PENDING_SESSIONS {
            return Ok(rejected_init("busy"));
        }

        let mut qr_guard = self.active_qr.write().await;
        let Some(session) = qr_guard.as_mut() else {
            return Ok(rejected_init("invalid_invite"));
        };
        if session.used {
            return Ok(rejected_init("invalid_invite"));
        }
        if session.expires_at <= Utc::now() {
            return Ok(rejected_init("invalid_invite"));
        }
        if session.token_b64 != token {
            return Ok(rejected_init("invalid_invite"));
        }
        session.used = true;
        let bound_profile_id = session.bound_profile_id.clone();

        let mut server_nonce = [0u8; 32];
        OsRng.fill_bytes(&mut server_nonce);
        let session_id = Uuid::new_v4();
        pending_sessions.insert(
            session_id,
            PendingPairSession {
                phone_id: request.phone_id.clone(),
                phone_name: request.phone_name.clone(),
                phone_public_key: phone_key,
                role: PairingRole::parse(request.role.as_deref()),
                bound_profile_id,
                server_nonce,
                created_at: Instant::now(),
            },
        );

        Ok(PairInitResponse {
            status: "challenge".to_string(),
            server_nonce: Some(base64url_encode(&server_nonce)),
            session_id: Some(session_id.to_string()),
            reason: None,
        })
    }

    pub async fn pair_verify(
        &self,
        request: PairVerifyRequest,
        source_ip: IpAddr,
    ) -> Result<PairVerifyResponse> {
        if !self
            .admission
            .lock()
            .await
            .allow_verify(source_ip, Instant::now())
        {
            return Ok(rejected_verify("rate_limited"));
        }
        let session_id = Uuid::parse_str(&request.session_id).context("invalid session_id")?;
        let pending = self.pending_sessions.write().await.remove(&session_id);
        let Some(pending) = pending else {
            return Ok(rejected_verify("invalid_session"));
        };
        if pending.created_at.elapsed() > VERIFY_TTL {
            return Ok(rejected_verify("invalid_session"));
        }

        let server_nonce_b64 = base64url_encode(&pending.server_nonce);
        verify_message(
            &pending.phone_public_key,
            &server_nonce_b64,
            &request.signed_nonce,
        )
        .context("phone nonce signature invalid")?;

        let phone_nonce = base64url_decode(&request.phone_nonce)?;
        if phone_nonce.len() != 32 {
            bail!("phone_nonce must be 32 bytes");
        }
        let phone_nonce_b64 = base64url_encode(&phone_nonce);
        let server_signed_nonce = sign_message(self.identity.signing_key(), &phone_nonce_b64);

        let session_token = Uuid::new_v4().to_string();
        let pairing_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let session_expires_at = now + SESSION_TOKEN_TTL;
        let record = PairedDeviceRecord {
            pairing_id: pairing_id.clone(),
            phone_id: pending.phone_id.clone(),
            phone_name: pending.phone_name.clone(),
            phone_public_key: verifying_key_to_b64(&pending.phone_public_key),
            paired_at: now,
            last_seen: now,
            session_token_hash: hash_session_token(&session_token),
            session_token_expiry: session_expires_at,
            trust_expires_at: None,
            idle_timeout_seconds: None,
            credential_generation: 1,
            role: pending.role,
            profile_id: pending.bound_profile_id,
            mesh_grants: crate::mesh::default_mesh_grants_for_role(pending.role),
            apns_device_token: None,
            push_platform: None,
            push_updated_at: None,
            live_activity_push_token: None,
            live_activity_push_updated_at: None,
        };
        self.store.save_record(&record)?;
        let _ = crate::mesh::registry::upsert_from_pairing(&record);

        Ok(PairVerifyResponse {
            status: "paired".to_string(),
            server_signed_nonce: Some(server_signed_nonce),
            session_token: Some(session_token),
            pairing_id: Some(pairing_id),
            session_expires_at: Some(session_expires_at),
            reason: None,
        })
    }

    /// Begin a session rotation using the durable device key established at pairing time.
    /// This route intentionally does not require the old bearer: an expired access token
    /// must not turn a still-trusted device into an unpaired device.
    pub async fn pair_session_challenge(
        &self,
        request: PairSessionChallengeRequest,
        source_ip: IpAddr,
    ) -> Result<PairSessionChallengeResponse> {
        if !self
            .admission
            .lock()
            .await
            .allow_refresh(source_ip, Instant::now())
        {
            return Ok(rejected_session_challenge("rate_limited"));
        }

        let pairing_id = request.pairing_id.trim();
        let phone_id = request.phone_id.trim();
        if pairing_id.is_empty() || phone_id.is_empty() {
            return Ok(rejected_session_challenge("invalid_pairing"));
        }
        let Some(record) = self.store.get_by_phone_id(phone_id)? else {
            return Ok(rejected_session_challenge("invalid_pairing"));
        };
        if record.pairing_id != pairing_id || !pairing_trust_active(&record, Utc::now()) {
            return Ok(rejected_session_challenge("invalid_pairing"));
        }

        let mut pending_sessions = self.pending_refresh_sessions.write().await;
        pending_sessions.retain(|_, pending| pending.created_at.elapsed() <= SESSION_REFRESH_TTL);
        if pending_sessions.len() >= MAX_PENDING_REFRESH_SESSIONS {
            return Ok(rejected_session_challenge("busy"));
        }

        let mut server_nonce = [0u8; 32];
        OsRng.fill_bytes(&mut server_nonce);
        let session_id = Uuid::new_v4();
        pending_sessions.insert(
            session_id,
            PendingRefreshSession {
                pairing_id: pairing_id.to_string(),
                phone_id: phone_id.to_string(),
                server_nonce,
                created_at: Instant::now(),
            },
        );

        Ok(PairSessionChallengeResponse {
            status: "challenge".to_string(),
            session_id: Some(session_id.to_string()),
            server_nonce: Some(base64url_encode(&server_nonce)),
            expires_at: Some(Utc::now() + SESSION_REFRESH_TTL),
            reason: None,
        })
    }

    /// Rotate an access token after proof of possession of the paired device key.
    pub async fn pair_session_refresh(
        &self,
        request: PairSessionRefreshRequest,
        source_ip: IpAddr,
    ) -> Result<PairSessionRefreshResponse> {
        if !self
            .admission
            .lock()
            .await
            .allow_refresh(source_ip, Instant::now())
        {
            return Ok(rejected_session_refresh("rate_limited"));
        }

        let Ok(session_id) = Uuid::parse_str(request.session_id.trim()) else {
            return Ok(rejected_session_refresh("invalid_session"));
        };
        let pending = self
            .pending_refresh_sessions
            .write()
            .await
            .remove(&session_id);
        let Some(pending) = pending else {
            return Ok(rejected_session_refresh("invalid_session"));
        };
        if pending.created_at.elapsed() > SESSION_REFRESH_TTL
            || pending.pairing_id != request.pairing_id.trim()
            || pending.phone_id != request.phone_id.trim()
        {
            return Ok(rejected_session_refresh("invalid_session"));
        }

        let _rotation = self.credential_rotation.lock().await;
        let Some(mut record) = self.store.get_by_phone_id(&pending.phone_id)? else {
            return Ok(rejected_session_refresh("invalid_pairing"));
        };
        let now = Utc::now();
        if record.pairing_id != pending.pairing_id || !pairing_trust_active(&record, now) {
            return Ok(rejected_session_refresh("invalid_pairing"));
        }

        let Ok(phone_nonce) = base64url_decode(request.phone_nonce.trim()) else {
            return Ok(rejected_session_refresh("invalid_proof"));
        };
        if phone_nonce.len() != 32 {
            return Ok(rejected_session_refresh("invalid_proof"));
        }
        let phone_nonce_b64 = base64url_encode(&phone_nonce);

        let phone_public_key = parse_verifying_key(&record.phone_public_key)?;
        let challenge_message = session_refresh_challenge_message(
            &session_id.to_string(),
            &record.pairing_id,
            &record.phone_id,
            &base64url_encode(&pending.server_nonce),
            &phone_nonce_b64,
        );
        if verify_message(
            &phone_public_key,
            &challenge_message,
            request.signed_nonce.trim(),
        )
        .is_err()
        {
            return Ok(rejected_session_refresh("invalid_proof"));
        }

        let session_token = Uuid::new_v4().to_string();
        let session_expires_at = now + SESSION_TOKEN_TTL;
        let old_generation = record.credential_generation;
        record.credential_generation = old_generation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("pairing credential generation exhausted"))?;
        record.session_token_hash = hash_session_token(&session_token);
        record.session_token_expiry = session_expires_at;
        record.last_seen = now;
        self.store.save_record(&record)?;
        let _ = crate::mesh::registry::upsert_from_pairing(&record);

        self.credential_lifecycle.revoke(
            record.pairing_id.clone(),
            old_generation,
            CredentialKind::Pairing,
            "pairing_session_rotated",
        );
        self.credential_lifecycle.record_rotation(
            record.pairing_id.clone(),
            record.credential_generation,
            CredentialKind::Pairing,
        );

        let issued_message = session_refresh_issued_message(
            &session_id.to_string(),
            &record.pairing_id,
            &phone_nonce_b64,
            &session_token,
            session_expires_at.timestamp(),
        );
        let server_signed_nonce = sign_message(self.identity.signing_key(), &issued_message);

        Ok(PairSessionRefreshResponse {
            status: "refreshed".to_string(),
            server_signed_nonce: Some(server_signed_nonce),
            session_token: Some(session_token),
            session_expires_at: Some(session_expires_at),
            reason: None,
        })
    }

    pub async fn pair_heartbeat(
        &self,
        credential_id: Option<&str>,
        body: Option<PairHeartbeatRequest>,
    ) -> Result<PairHeartbeatResponse> {
        let Some(credential_id) = credential_id else {
            return Ok(PairHeartbeatResponse {
                status: "unauthorized".to_string(),
                device_time: Utc::now(),
                reason: Some("missing_token".to_string()),
            });
        };
        let record = self
            .store
            .list_paired()?
            .into_iter()
            .find(|record| record.pairing_id == credential_id);
        let Some(record) = record else {
            return Ok(PairHeartbeatResponse {
                status: "unauthorized".to_string(),
                device_time: Utc::now(),
                reason: Some("invalid_token".to_string()),
            });
        };
        if record.session_token_expiry <= Utc::now()
            || !pairing_trust_active(&record, Utc::now())
        {
            return Ok(PairHeartbeatResponse {
                status: "unauthorized".to_string(),
                device_time: Utc::now(),
                reason: Some("expired".to_string()),
            });
        }

        let mut updated = record.clone();
        let mut mesh_endpoints = None;
        if let Some(body) = body {
            if let Some(push_token) = body
                .apns_device_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                updated.apns_device_token = Some(push_token.to_string());
                updated.push_platform = body
                    .push_platform
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| Some("ios".to_string()));
                updated.push_updated_at = Some(Utc::now());
            }
            if let Some(live_token) = body.live_activity_push_token.as_deref() {
                let trimmed = live_token.trim();
                if trimmed.is_empty() {
                    updated.live_activity_push_token = None;
                    updated.live_activity_push_updated_at = None;
                } else {
                    updated.live_activity_push_token = Some(trimmed.to_string());
                    updated.live_activity_push_updated_at = Some(Utc::now());
                }
            }
            let lan = body
                .mesh_lan_base_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let ticket = body
                .mesh_iroh_ticket
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let endpoint = body
                .mesh_iroh_endpoint_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            if lan.is_some() || ticket.is_some() || endpoint.is_some() {
                mesh_endpoints = Some(crate::mesh::MeshPeerEndpoints {
                    lan_base_url: lan,
                    iroh_ticket: ticket,
                    iroh_endpoint_id: endpoint,
                });
            }
        }
        updated.last_seen = Utc::now();
        self.store.save_record(&updated)?;
        let _ = crate::mesh::registry::upsert_from_pairing(&updated);
        if let Some(endpoints) = mesh_endpoints {
            let _ = crate::mesh::registry::set_endpoints(&updated.phone_id, endpoints);
        }

        Ok(PairHeartbeatResponse {
            status: "ok".to_string(),
            device_time: Utc::now(),
            reason: None,
        })
    }

    pub fn list_apns_targets(&self) -> Result<Vec<ApnsPushTarget>> {
        let mut out = Vec::new();
        for record in self.store.list_paired()? {
            if !pairing_trust_active(&record, Utc::now()) {
                continue;
            }
            let Some(token) = record
                .apns_device_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if record
                .push_platform
                .as_deref()
                .is_some_and(|platform| platform != "ios")
            {
                continue;
            }
            out.push(ApnsPushTarget {
                phone_id: record.phone_id,
                device_token: token.to_string(),
            });
        }
        Ok(out)
    }

    pub fn list_live_activity_targets(&self) -> Result<Vec<LiveActivityPushTarget>> {
        let mut out = Vec::new();
        for record in self.store.list_paired()? {
            if !pairing_trust_active(&record, Utc::now()) {
                continue;
            }
            let Some(token) = record
                .live_activity_push_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            out.push(LiveActivityPushTarget {
                phone_id: record.phone_id,
                push_token: token.to_string(),
            });
        }
        Ok(out)
    }

    pub async fn revoke_pairing(
        &self,
        pairing_id: &str,
        authority: RevokePairingAuthority<'_>,
    ) -> Result<RevokePairingResult> {
        let authorized = match authority {
            RevokePairingAuthority::Administrator => true,
            RevokePairingAuthority::Credential(credential_id) => credential_id == pairing_id,
            RevokePairingAuthority::Unauthenticated => false,
        };
        if !authorized {
            return Ok(RevokePairingResult::Unauthorized);
        }
        let paired = self.store.list_paired()?;
        let Some(record) = paired
            .into_iter()
            .find(|entry| entry.pairing_id == pairing_id)
        else {
            return Ok(RevokePairingResult::NotFound);
        };
        self.store.revoke_pairing(pairing_id)?;
        self.store.delete_record(&record.phone_id)?;
        self.credential_lifecycle.revoke(
            record.pairing_id,
            record.credential_generation,
            CredentialKind::Pairing,
            "pairing_revoked",
        );
        Ok(RevokePairingResult::Removed)
    }

    pub fn render_qr_png(&self, url: &str) -> Result<Vec<u8>> {
        use image::Luma;
        use qrcode::EcLevel;
        use qrcode::QrCode;

        let code = QrCode::with_error_correction_level(url.as_bytes(), EcLevel::M)
            .or_else(|_| QrCode::new(url.as_bytes()))
            .with_context(|| format!("build qr code ({} bytes)", url.len()))?;
        let image = code.render::<Luma<u8>>().min_dimensions(256, 256).build();
        let mut buffer = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buffer);
        image
            .write_to(&mut cursor, image::ImageFormat::Png)
            .context("encode qr png")?;
        Ok(buffer)
    }

    fn build_qr_session(&self, bound_profile_id: Option<String>) -> Result<ActiveQrSession> {
        let session_key = SigningKey::generate(&mut OsRng);
        let token_b64 = base64url_encode(session_key.verifying_key().as_bytes());
        let challenge = Sha256::digest(format!(
            "{}|{}|{}",
            self.identity.device_id,
            token_b64,
            Utc::now().timestamp()
        ));
        let short_code_raw = encode_short_code(&challenge);
        Ok(ActiveQrSession {
            token_b64: token_b64.clone(),
            short_code: format_short_code(&short_code_raw),
            short_code_raw,
            expires_at: Utc::now() + QR_TTL,
            used: false,
            bound_profile_id,
        })
    }

    fn build_qr_url(&self, session: &ActiveQrSession, full: bool) -> Result<String> {
        let name = urlencoding::encode(&self.peer_name);
        let address = urlencoding::encode(&self.advertise_address);
        let daemon_public_key = verifying_key_to_b64(self.identity.verifying_key());
        let daemon_public_key = urlencoding::encode(&daemon_public_key);
        let profile_param = session
            .bound_profile_id
            .as_deref()
            .map(|profile_id| format!("&p={}", urlencoding::encode(profile_id)))
            .unwrap_or_default();
        let profile_for_sig = session.bound_profile_id.as_deref();

        // Compact v1 is the default for camera / Messages. Full v2 embeds the Iroh ticket
        // (large) and is only for explicit paste/share when off-LAN bootstrap is required.
        if full && self.can_emit_qr_v2() {
            let iroh = self
                .iroh
                .as_ref()
                .expect("iroh workshop info required for qr v2");
            let message = qr_signing_message_v2(
                &self.advertise_address,
                &self.identity.device_id,
                &session.token_b64,
                &iroh.ticket,
                profile_for_sig,
            );
            let signature = sign_message(self.identity.signing_key(), &message);
            let ticket = urlencoding::encode(&iroh.ticket);
            let endpoint_id = urlencoding::encode(&iroh.endpoint_id);
            return Ok(format!(
                "{QR_SCHEME_V2}?a={address}&d={}&t={}&s={signature}&n={name}&u={daemon_public_key}&k={ticket}&e={endpoint_id}{profile_param}",
                self.identity.device_id, session.token_b64,
            ));
        }

        let message = qr_signing_message(
            &self.advertise_address,
            &self.identity.device_id,
            &session.token_b64,
            profile_for_sig,
        );
        let signature = sign_message(self.identity.signing_key(), &message);
        Ok(format!(
            "{QR_SCHEME}?a={address}&d={}&t={}&s={signature}&n={name}&u={daemon_public_key}{profile_param}",
            self.identity.device_id, session.token_b64,
        ))
    }

    fn can_emit_qr_v2(&self) -> bool {
        self.iroh.is_some() && !pairing_qr_v1_from_env()
    }

    async fn resolve_short_code(&self, code: &str) -> Result<String> {
        let normalized = code.replace('-', "").to_ascii_uppercase();
        let guard = self.active_qr.read().await;
        let Some(session) = guard.as_ref() else {
            bail!("no active qr session");
        };
        if session.used {
            bail!("token already used");
        }
        if session.expires_at <= Utc::now() {
            bail!("token expired");
        }
        if normalized != session.short_code_raw {
            bail!("invalid short code");
        }
        Ok(session.token_b64.clone())
    }

    pub fn find_by_session_token(&self, token: &str) -> Result<Option<PairedDeviceRecord>> {
        let hash = hash_session_token(token);
        for record in self.store.list_paired()? {
            if record.session_token_hash == hash {
                return Ok(Some(record));
            }
        }
        Ok(None)
    }

    pub fn find_by_phone_id(&self, phone_id: &str) -> Result<Option<PairedDeviceRecord>> {
        Ok(self
            .store
            .get_by_phone_id(phone_id)?
            .filter(|record| pairing_trust_active(record, Utc::now())))
    }

    /// Resolve an already-authenticated opaque credential ID to its current
    /// pairing record. Session expiry is checked before this lookup, while the
    /// durable device trust policy remains authoritative here.
    pub fn find_by_pairing_id(&self, pairing_id: &str) -> Result<Option<PairedDeviceRecord>> {
        Ok(self
            .store
            .list_paired()?
            .into_iter()
            .find(|record| record.pairing_id == pairing_id)
            .filter(|record| pairing_trust_active(record, Utc::now())))
    }

    /// Replace the durable trust policy for one paired device. Any policy change
    /// expires the current bearer so the device must prove possession of its
    /// durable key before receiving another session.
    pub async fn update_trust_policy(
        &self,
        pairing_id: &str,
        request: PairTrustPolicyUpdateRequest,
    ) -> Result<Option<PairedDeviceSummary>> {
        if let Some(timeout) = request.idle_timeout_seconds
            && !(MIN_IDLE_TIMEOUT_SECONDS..=MAX_IDLE_TIMEOUT_SECONDS).contains(&timeout)
        {
            bail!(
                "idleTimeoutSeconds must be between {MIN_IDLE_TIMEOUT_SECONDS} and {MAX_IDLE_TIMEOUT_SECONDS}"
            );
        }
        let now = Utc::now();
        if request
            .trust_expires_at
            .is_some_and(|expires_at| expires_at <= now)
        {
            bail!("trustExpiresAt must be in the future");
        }

        let _rotation = self.credential_rotation.lock().await;
        let Some(mut record) = self
            .store
            .list_paired()?
            .into_iter()
            .find(|record| record.pairing_id == pairing_id)
        else {
            return Ok(None);
        };
        let old_generation = record.credential_generation;
        record.credential_generation = old_generation
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("pairing credential generation exhausted"))?;
        record.session_token_expiry = now;
        record.trust_expires_at = request.trust_expires_at;
        record.idle_timeout_seconds = request.idle_timeout_seconds;
        self.store.save_record(&record)?;
        let _ = crate::mesh::registry::upsert_from_pairing(&record);
        self.credential_lifecycle.revoke(
            record.pairing_id.clone(),
            old_generation,
            CredentialKind::Pairing,
            "pairing_trust_policy_changed",
        );
        self.credential_lifecycle.record_rotation(
            record.pairing_id.clone(),
            record.credential_generation,
            CredentialKind::Pairing,
        );
        Ok(Some(paired_device_summary(record, now)))
    }

    /// Persist mesh grants on the pairing record and refresh the mesh registry projection.
    pub fn set_mesh_grants(
        &self,
        phone_id: &str,
        grants: Vec<String>,
    ) -> Result<PairedDeviceRecord> {
        let mut record = self
            .store
            .get_by_phone_id(phone_id)?
            .ok_or_else(|| anyhow::anyhow!("paired device not found: {phone_id}"))?;
        record.mesh_grants = grants
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .collect();
        self.store.save_record(&record)?;
        let _ = crate::mesh::registry::upsert_from_pairing(&record);
        Ok(record)
    }

    /// Returns the paired device when the bearer is valid and unexpired.
    pub fn resolve_bearer_record(&self, token: &str) -> Result<Option<PairedDeviceRecord>> {
        let Some(record) = self.find_by_session_token(token)? else {
            return Ok(None);
        };
        if record.session_token_expiry <= Utc::now() {
            return Ok(None);
        }
        if !pairing_trust_active(&record, Utc::now()) {
            return Ok(None);
        }
        if self.store.is_revoked(&record.pairing_id)? {
            return Ok(None);
        }
        Ok(Some(record))
    }

    /// Shared-mode seat bound to this bearer, when present.
    pub fn resolve_bearer_profile_id(&self, token: &str) -> Result<Option<String>> {
        Ok(self
            .resolve_bearer_record(token)?
            .and_then(|record| record.profile_id)
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty()))
    }
}

fn paired_device_summary(record: PairedDeviceRecord, now: DateTime<Utc>) -> PairedDeviceSummary {
    let trust_active = pairing_trust_active(&record, now);
    PairedDeviceSummary {
        pairing_id: record.pairing_id,
        phone_id: record.phone_id,
        phone_name: record.phone_name,
        paired_at: record.paired_at,
        last_seen: record.last_seen,
        session_expires_at: record.session_token_expiry,
        trust_expires_at: record.trust_expires_at,
        idle_timeout_seconds: record.idle_timeout_seconds,
        trust_active,
        role: record.role.as_str().to_string(),
        profile_id: record.profile_id,
    }
}

fn pairing_trust_active(record: &PairedDeviceRecord, now: DateTime<Utc>) -> bool {
    if record
        .trust_expires_at
        .is_some_and(|expires_at| expires_at <= now)
    {
        return false;
    }
    let Some(idle_timeout_seconds) = record.idle_timeout_seconds else {
        return true;
    };
    let idle_seconds = now
        .signed_duration_since(record.last_seen)
        .num_seconds()
        .max(0) as u64;
    idle_seconds < idle_timeout_seconds
}

fn session_refresh_challenge_message(
    session_id: &str,
    pairing_id: &str,
    phone_id: &str,
    server_nonce: &str,
    phone_nonce: &str,
) -> String {
    format!(
        "medousa-session-refresh-v1|{session_id}|{pairing_id}|{phone_id}|{server_nonce}|{phone_nonce}"
    )
}

fn session_refresh_issued_message(
    session_id: &str,
    pairing_id: &str,
    phone_nonce: &str,
    session_token: &str,
    session_expires_at_unix: i64,
) -> String {
    format!(
        "medousa-session-issued-v1|{session_id}|{pairing_id}|{phone_nonce}|{session_token}|{session_expires_at_unix}"
    )
}

#[cfg(test)]
mod peer_role_tests {
    use super::PairingRole;

    #[test]
    fn pairing_role_defaults_to_portal() {
        assert_eq!(PairingRole::parse(None), PairingRole::Portal);
        assert_eq!(PairingRole::parse(Some("portal")), PairingRole::Portal);
        assert_eq!(PairingRole::parse(Some("peer")), PairingRole::Peer);
    }
}

fn rejected_init(reason: &str) -> PairInitResponse {
    PairInitResponse {
        status: "rejected".to_string(),
        server_nonce: None,
        session_id: None,
        reason: Some(reason.to_string()),
    }
}

fn allow_attempt(
    global: &mut VecDeque<Instant>,
    by_source: &mut HashMap<IpAddr, VecDeque<Instant>>,
    source: IpAddr,
    now: Instant,
    window: Duration,
    global_limit: usize,
    source_limit: usize,
) -> bool {
    global.retain(|attempt| now.duration_since(*attempt) < window);
    by_source.retain(|_, attempts| {
        attempts.retain(|attempt| now.duration_since(*attempt) < window);
        !attempts.is_empty()
    });
    let source_attempts = by_source.entry(source).or_default();
    if global.len() >= global_limit || source_attempts.len() >= source_limit {
        return false;
    }
    global.push_back(now);
    source_attempts.push_back(now);
    true
}

fn rejected_verify(reason: &str) -> PairVerifyResponse {
    PairVerifyResponse {
        status: "rejected".to_string(),
        server_signed_nonce: None,
        session_token: None,
        pairing_id: None,
        session_expires_at: None,
        reason: Some(reason.to_string()),
    }
}

fn rejected_session_challenge(reason: &str) -> PairSessionChallengeResponse {
    PairSessionChallengeResponse {
        status: "rejected".to_string(),
        session_id: None,
        server_nonce: None,
        expires_at: None,
        reason: Some(reason.to_string()),
    }
}

fn rejected_session_refresh(reason: &str) -> PairSessionRefreshResponse {
    PairSessionRefreshResponse {
        status: "rejected".to_string(),
        server_signed_nonce: None,
        session_token: None,
        session_expires_at: None,
        reason: Some(reason.to_string()),
    }
}

fn encode_short_code(digest: &[u8]) -> String {
    let mut out = String::with_capacity(6);
    for &byte in digest.iter().take(6) {
        let slot = (byte as usize) % SHORT_CODE_ALPHABET.len();
        out.push(SHORT_CODE_ALPHABET[slot] as char);
    }
    out
}

fn format_short_code(raw: &str) -> String {
    format!("{}-{}-{}", &raw[0..3], &raw[3..5], &raw[5..6])
}

pub fn resolve_peer_name() -> String {
    std::env::var("MEDOUSA_PEER_NAME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Medousa Workshop".to_string())
}

pub fn resolve_advertise_address(bind: &str) -> String {
    let port = crate::daemon_api::parse_daemon_bind_port(bind);
    if bind.starts_with("0.0.0.0:") || bind.starts_with("[::]:") {
        crate::daemon_api::detect_lan_ipv4()
            .map(|host| format!("{host}:{port}"))
            .unwrap_or_else(|| format!("127.0.0.1:{port}"))
    } else {
        format!("127.0.0.1:{port}")
    }
}

pub fn pairing_enabled_from_env() -> bool {
    !truthy_env("MEDOUSA_PAIRING_DISABLE")
}

pub fn pairing_qr_v1_from_env() -> bool {
    truthy_env("MEDOUSA_PAIRING_QR_V1")
}

pub fn mdns_enabled_from_env() -> bool {
    pairing_enabled_from_env() && !truthy_env("MEDOUSA_MDNS_DISABLE")
}

pub fn mdns_should_advertise(bind: &str) -> bool {
    mdns_enabled_from_env()
        && (bind.starts_with("0.0.0.0:")
            || bind.starts_with("[::]:")
            || truthy_env("MEDOUSA_PAIRING_ADVERTISE"))
}

fn truthy_env(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn test_service() -> Arc<PairingService> {
        Arc::new(PairingService::new(
            DeviceIdentity::generate_ephemeral(),
            "127.0.0.1:7419".to_string(),
            "Test Workshop".to_string(),
            Some("llama3.2:3b".to_string()),
            None,
        ))
    }

    fn test_service_with_iroh() -> Arc<PairingService> {
        Arc::new(PairingService::new(
            DeviceIdentity::generate_ephemeral(),
            "127.0.0.1:7419".to_string(),
            "Test Workshop".to_string(),
            Some("llama3.2:3b".to_string()),
            Some(IrohWorkshopInfo {
                ticket: "test-iroh-ticket".to_string(),
                endpoint_id: "abc123".repeat(8),
            }),
        ))
    }

    async fn pair_test_phone(
        service: &PairingService,
        role: Option<&str>,
    ) -> (String, String, String, SigningKey) {
        let qr = service.current_qr().await.expect("qr");
        let token = extract_query_param(&qr.url, "t").expect("token");
        let phone = SigningKey::generate(&mut OsRng);
        let phone_id = format!("phone-test-{}", Uuid::new_v4());
        let init = service
            .pair_init(
                PairInitRequest {
                    qr_token: Some(token),
                    short_code: None,
                    phone_id: phone_id.clone(),
                    phone_name: "Test Phone".to_string(),
                    public_key: verifying_key_to_b64(&phone.verifying_key()),
                    role: role.map(str::to_string),
                },
                "127.0.0.1".parse().expect("source ip"),
            )
            .await
            .expect("init");
        assert_eq!(init.status, "challenge");
        let signed_nonce =
            sign_message(&phone, init.server_nonce.as_deref().expect("server nonce"));
        let mut phone_nonce = [0u8; 32];
        OsRng.fill_bytes(&mut phone_nonce);
        let verify = service
            .pair_verify(
                PairVerifyRequest {
                    session_id: init.session_id.expect("session"),
                    signed_nonce,
                    phone_nonce: base64url_encode(&phone_nonce),
                },
                "127.0.0.1".parse().expect("source ip"),
            )
            .await
            .expect("verify");
        assert_eq!(verify.status, "paired");
        (
            verify.pairing_id.expect("pairing"),
            verify.session_token.expect("session token"),
            phone_id,
            phone,
        )
    }

    #[tokio::test]
    async fn qr_url_contains_device_id() {
        let service = test_service();
        let qr = service.current_qr().await.expect("qr");
        assert!(qr.url.contains("medousa://pair/1.0"));
        assert!(qr.url.contains(&service.device_id().to_string()));
        let public_key = extract_query_param(&qr.url, "u").expect("public key");
        let address = extract_query_param(&qr.url, "a").expect("address");
        let device_id = extract_query_param(&qr.url, "d").expect("device id");
        let token = extract_query_param(&qr.url, "t").expect("token");
        let signature = extract_query_param(&qr.url, "s").expect("signature");
        let verifying_key = parse_verifying_key(&public_key).expect("verifying key");
        crate::pairing::crypto::verify_qr_url_signature(
            &verifying_key,
            &address,
            &device_id,
            &token,
            &signature,
        )
        .expect("QR signature verifies with embedded key");
    }

    #[tokio::test]
    async fn daemon_startup_has_no_active_pairing_window() {
        let service = test_service();
        assert!(!service.pair_status().await.expect("status").qr_active);

        let phone = SigningKey::generate(&mut OsRng);
        let response = service
            .pair_init(
                PairInitRequest {
                    qr_token: Some("unissued".to_string()),
                    short_code: None,
                    phone_id: "phone-window-test".to_string(),
                    phone_name: "Phone".to_string(),
                    public_key: verifying_key_to_b64(&phone.verifying_key()),
                    role: None,
                },
                "127.0.0.1".parse().expect("source ip"),
            )
            .await
            .expect("init response");
        assert_eq!(response.reason.as_deref(), Some("invalid_invite"));
    }

    #[tokio::test]
    async fn pairing_admission_is_bounded_per_source_and_globally() {
        let service = test_service();
        let phone = SigningKey::generate(&mut OsRng);
        let request = PairInitRequest {
            qr_token: Some("invalid".to_string()),
            short_code: None,
            phone_id: "phone-rate-test".to_string(),
            phone_name: "Phone".to_string(),
            public_key: verifying_key_to_b64(&phone.verifying_key()),
            role: None,
        };
        let source = "127.0.0.1".parse().expect("source ip");
        for _ in 0..INIT_RATE_LIMIT {
            assert_ne!(
                service
                    .pair_init(request.clone(), source)
                    .await
                    .expect("init")
                    .reason
                    .as_deref(),
                Some("rate_limited")
            );
        }
        assert_eq!(
            service
                .pair_init(request.clone(), source)
                .await
                .expect("init")
                .reason
                .as_deref(),
            Some("rate_limited")
        );

        let service = test_service();
        for index in 1..=GLOBAL_INIT_RATE_LIMIT {
            let source = IpAddr::from([10, 0, 0, index as u8]);
            assert_ne!(
                service
                    .pair_init(request.clone(), source)
                    .await
                    .expect("init")
                    .reason
                    .as_deref(),
                Some("rate_limited")
            );
        }
        assert_eq!(
            service
                .pair_init(request, IpAddr::from([10, 0, 1, 1]))
                .await
                .expect("init")
                .reason
                .as_deref(),
            Some("rate_limited")
        );
    }

    #[test]
    fn pairing_concurrency_is_bounded() {
        let service = test_service();
        let permits = (0..CEREMONY_CONCURRENCY)
            .map(|_| service.try_acquire_ceremony().expect("permit"))
            .collect::<Vec<_>>();
        assert!(service.try_acquire_ceremony().is_none());
        drop(permits);
        assert!(service.try_acquire_ceremony().is_some());
    }

    #[tokio::test]
    async fn default_qr_stays_compact_when_iroh_available() {
        let service = test_service_with_iroh();
        let qr = service.current_qr().await.expect("qr");
        assert!(qr.url.contains("medousa://pair/1.0"));
        assert!(!qr.url.contains("k="));
    }

    #[tokio::test]
    async fn full_qr_v2_url_contains_iroh_ticket() {
        let service = test_service_with_iroh();
        let qr = service
            .current_qr_with_options(true)
            .await
            .expect("full qr");
        assert!(qr.url.contains("medousa://pair/2.0"));
        assert!(qr.url.contains("k=test-iroh-ticket"));
        assert!(qr.url.contains("e="));
    }

    #[test]
    fn capability_flags_set_relay_bit_with_iroh() {
        let service = test_service_with_iroh();
        assert_eq!(service.capability_flags(), "003F");
    }

    #[tokio::test]
    async fn token_replay_rejected() {
        let service = test_service();
        let qr = service.current_qr().await.expect("qr");
        let token = extract_query_param(&qr.url, "t").expect("token");
        let phone = SigningKey::generate(&mut OsRng);
        let init = PairInitRequest {
            qr_token: Some(token.clone()),
            short_code: None,
            phone_id: "phone0001".to_string(),
            phone_name: "Phone A".to_string(),
            public_key: verifying_key_to_b64(&phone.verifying_key()),
            role: None,
        };
        let source = "127.0.0.1".parse().expect("source ip");
        let first = service.pair_init(init.clone(), source).await.expect("init");
        assert_eq!(first.status, "challenge");
        let second = service.pair_init(init, source).await.expect("init");
        assert_eq!(second.status, "rejected");
        assert_eq!(second.reason.as_deref(), Some("invalid_invite"));
    }

    #[tokio::test]
    async fn full_pairing_handshake() {
        let service = test_service();
        let (pairing_id, session_token, phone_id, _) = pair_test_phone(&service, None).await;
        assert!(
            service
                .resolve_bearer_record(&session_token)
                .unwrap()
                .is_some()
        );
        assert!(service.find_by_pairing_id(&pairing_id).unwrap().is_some());
        assert_eq!(
            service
                .pair_heartbeat(Some(&pairing_id), None)
                .await
                .expect("heartbeat")
                .status,
            "ok"
        );
        assert_eq!(
            service
                .pair_heartbeat(Some(&session_token), None)
                .await
                .expect("raw bearer is not an internal credential id")
                .status,
            "unauthorized"
        );
        service.store.delete_record(&phone_id).expect("cleanup");
    }

    #[tokio::test]
    async fn expired_session_token_is_rejected() {
        let service = test_service();
        let (pairing_id, session_token, phone_id, _) = pair_test_phone(&service, None).await;
        let mut record = service
            .find_by_phone_id(&phone_id)
            .expect("read pairing")
            .expect("pairing record");
        record.session_token_expiry = Utc::now() - chrono::Duration::seconds(1);
        service.store.save_record(&record).expect("expire pairing");

        assert!(
            service
                .resolve_bearer_record(&session_token)
                .unwrap()
                .is_none()
        );
        assert!(service.find_by_pairing_id(&pairing_id).unwrap().is_some());
        service.store.delete_record(&phone_id).expect("cleanup");
    }

    #[tokio::test]
    async fn expired_session_rotates_with_the_durable_device_key() {
        let service = test_service();
        let (pairing_id, old_token, phone_id, phone_key) =
            pair_test_phone(&service, None).await;
        let mut record = service
            .store
            .get_by_phone_id(&phone_id)
            .expect("read pairing")
            .expect("pairing record");
        record.session_token_expiry = Utc::now() - chrono::Duration::seconds(1);
        service.store.save_record(&record).expect("expire session");
        assert!(service.resolve_bearer_record(&old_token).unwrap().is_none());

        let challenge = service
            .pair_session_challenge(
                PairSessionChallengeRequest {
                    pairing_id: pairing_id.clone(),
                    phone_id: phone_id.clone(),
                },
                "127.0.0.1".parse().unwrap(),
            )
            .await
            .expect("challenge");
        assert_eq!(challenge.status, "challenge");
        let session_id = challenge.session_id.expect("session id");
        let server_nonce = challenge.server_nonce.expect("server nonce");
        let mut phone_nonce = [0_u8; 32];
        OsRng.fill_bytes(&mut phone_nonce);
        let phone_nonce = base64url_encode(&phone_nonce);
        let signed_nonce = sign_message(
            &phone_key,
            &session_refresh_challenge_message(
                &session_id,
                &pairing_id,
                &phone_id,
                &server_nonce,
                &phone_nonce,
            ),
        );
        let refreshed = service
            .pair_session_refresh(
                PairSessionRefreshRequest {
                    session_id: session_id.clone(),
                    pairing_id: pairing_id.clone(),
                    phone_id: phone_id.clone(),
                    signed_nonce,
                    phone_nonce: phone_nonce.clone(),
                },
                "127.0.0.1".parse().unwrap(),
            )
            .await
            .expect("refresh");
        assert_eq!(refreshed.status, "refreshed");
        let new_token = refreshed.session_token.expect("new token");
        let expires_at = refreshed.session_expires_at.expect("new expiry");
        verify_message(
            service.identity.verifying_key(),
            &session_refresh_issued_message(
                &session_id,
                &pairing_id,
                &phone_nonce,
                &new_token,
                expires_at.timestamp(),
            ),
            refreshed
                .server_signed_nonce
                .as_deref()
                .expect("server signature"),
        )
        .expect("server signs issued session");
        assert!(service.resolve_bearer_record(&old_token).unwrap().is_none());
        assert!(service.resolve_bearer_record(&new_token).unwrap().is_some());
        assert_eq!(
            service
                .find_by_phone_id(&phone_id)
                .unwrap()
                .expect("trusted pairing")
                .credential_generation,
            2
        );
        service.store.delete_record(&phone_id).expect("cleanup");
    }

    #[tokio::test]
    async fn expired_device_trust_cannot_refresh() {
        let service = test_service();
        let (pairing_id, _, phone_id, _) = pair_test_phone(&service, None).await;
        let mut record = service
            .store
            .get_by_phone_id(&phone_id)
            .unwrap()
            .expect("pairing");
        record.trust_expires_at = Some(Utc::now() - chrono::Duration::seconds(1));
        service.store.save_record(&record).expect("expire trust");

        let challenge = service
            .pair_session_challenge(
                PairSessionChallengeRequest {
                    pairing_id,
                    phone_id: phone_id.clone(),
                },
                "127.0.0.1".parse().unwrap(),
            )
            .await
            .expect("challenge response");
        assert_eq!(challenge.status, "rejected");
        assert_eq!(challenge.reason.as_deref(), Some("invalid_pairing"));
        assert!(service.find_by_phone_id(&phone_id).unwrap().is_none());
        service.store.delete_record(&phone_id).expect("cleanup");
    }

    #[tokio::test]
    async fn revoke_requires_matching_credential_authority() {
        let service = test_service();
        let (pairing_id, session_token, _, _) = pair_test_phone(&service, Some("portal")).await;

        assert_eq!(
            service
                .revoke_pairing(&pairing_id, RevokePairingAuthority::Unauthenticated)
                .await
                .expect("revoke"),
            RevokePairingResult::Unauthorized
        );

        assert_eq!(
            service
                .revoke_pairing(
                    &pairing_id,
                    RevokePairingAuthority::Credential(&session_token),
                )
                .await
                .expect("revoke"),
            RevokePairingResult::Unauthorized
        );

        assert_eq!(
            service
                .revoke_pairing(&pairing_id, RevokePairingAuthority::Credential(&pairing_id),)
                .await
                .expect("revoke"),
            RevokePairingResult::Removed
        );
        assert!(
            service
                .resolve_bearer_record(&session_token)
                .unwrap()
                .is_none()
        );
        assert!(service.find_by_pairing_id(&pairing_id).unwrap().is_none());
        let snapshot = service.credential_lifecycle().snapshot();
        assert_eq!(snapshot.revocation_epoch, 1);
        assert_eq!(snapshot.audit_events[0].credential_id, pairing_id);
    }

    fn extract_query_param(url: &str, key: &str) -> Option<String> {
        let query = url.split('?').nth(1)?;
        for pair in query.split('&') {
            let (name, value) = pair.split_once('=')?;
            if name == key {
                return urlencoding::decode(value)
                    .ok()
                    .map(|value| value.into_owned());
            }
        }
        None
    }
}
