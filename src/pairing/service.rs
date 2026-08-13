use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use base64::Engine;
use chrono::{DateTime, Utc};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::crypto::{
    PROTOCOL_VERSION, QR_SCHEME, QR_SCHEME_V2, base64url_decode, base64url_encode, hash_session_token,
    parse_verifying_key, qr_signing_message, qr_signing_message_v2, sign_message, verify_message,
    verifying_key_to_b64,
};
use super::identity::DeviceIdentity;
use super::store::{PairedDeviceRecord, PairingRole, PairingStore};

const QR_TTL: Duration = Duration::from_secs(300);
const VERIFY_TTL: Duration = Duration::from_secs(10);
const SESSION_TOKEN_TTL: Duration = Duration::from_secs(86_400);
const INIT_RATE_LIMIT: usize = 3;
const INIT_RATE_WINDOW: Duration = Duration::from_secs(60);

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
    pub reason: Option<String>,
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
    init_attempts: RwLock<HashMap<String, Vec<Instant>>>,
}

impl PairingService {
    pub fn new(
        identity: DeviceIdentity,
        advertise_address: String,
        peer_name: String,
        model_descriptor: Option<String>,
        iroh: Option<IrohWorkshopInfo>,
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
            init_attempts: RwLock::new(HashMap::new()),
        }
    }

    pub fn iroh_ticket(&self) -> Option<IrohTicketResponse> {
        self.iroh.as_ref().map(|info| IrohTicketResponse {
            ticket: info.ticket.clone(),
            endpoint_id: info.endpoint_id.clone(),
            available: true,
        })
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
                .map(|record| PairedDeviceSummary {
                    pairing_id: record.pairing_id,
                    phone_id: record.phone_id,
                    phone_name: record.phone_name,
                    paired_at: record.paired_at,
                    last_seen: record.last_seen,
                    role: record.role.as_str().to_string(),
                    profile_id: record.profile_id,
                })
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
        let needs_refresh = guard.as_ref().is_none_or(|session| {
            session.used || session.expires_at <= Utc::now()
        });
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
        source_ip: &str,
    ) -> Result<PairInitResponse> {
        if !self.allow_init_attempt(source_ip).await {
            return Ok(rejected_init("rate_limited"));
        }

        let phone_key = parse_verifying_key(&request.public_key)?;
        let token = if let Some(code) = request.short_code.as_deref() {
            self.resolve_short_code(code).await?
        } else if let Some(token) = request.qr_token.as_deref() {
            token.to_string()
        } else {
            return Ok(rejected_init("missing_token"));
        };

        let mut qr_guard = self.active_qr.write().await;
        let Some(session) = qr_guard.as_mut() else {
            return Ok(rejected_init("no_active_qr"));
        };
        if session.used {
            return Ok(rejected_init("token_already_used"));
        }
        if session.expires_at <= Utc::now() {
            return Ok(rejected_init("token_expired"));
        }
        if session.token_b64 != token {
            return Ok(rejected_init("invalid_token"));
        }
        session.used = true;
        let bound_profile_id = session.bound_profile_id.clone();

        let mut server_nonce = [0u8; 32];
        OsRng.fill_bytes(&mut server_nonce);
        let session_id = Uuid::new_v4();
        self.pending_sessions.write().await.insert(
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

    pub async fn pair_verify(&self, request: PairVerifyRequest) -> Result<PairVerifyResponse> {
        let session_id = Uuid::parse_str(&request.session_id)
            .context("invalid session_id")?;
        let pending = self
            .pending_sessions
            .write()
            .await
            .remove(&session_id);
        let Some(pending) = pending else {
            return Ok(rejected_verify("unknown_session"));
        };
        if pending.created_at.elapsed() > VERIFY_TTL {
            return Ok(rejected_verify("verify_timeout"));
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
        let server_signed_nonce =
            sign_message(self.identity.signing_key(), &phone_nonce_b64);

        let session_token = Uuid::new_v4().to_string();
        let pairing_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let record = PairedDeviceRecord {
            pairing_id: pairing_id.clone(),
            phone_id: pending.phone_id.clone(),
            phone_name: pending.phone_name.clone(),
            phone_public_key: verifying_key_to_b64(&pending.phone_public_key),
            paired_at: now,
            last_seen: now,
            session_token_hash: hash_session_token(&session_token),
            session_token_expiry: now + SESSION_TOKEN_TTL,
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
        if record.session_token_expiry < Utc::now() {
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
        let Some(record) = paired.into_iter().find(|entry| entry.pairing_id == pairing_id) else {
            return Ok(RevokePairingResult::NotFound);
        };
        self.store.revoke_pairing(pairing_id)?;
        self.store.delete_record(&record.phone_id)?;
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
                "{QR_SCHEME_V2}?a={address}&d={}&t={}&s={signature}&n={name}&k={ticket}&e={endpoint_id}{profile_param}",
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
            "{QR_SCHEME}?a={address}&d={}&t={}&s={signature}&n={name}{profile_param}",
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

    async fn allow_init_attempt(&self, source_ip: &str) -> bool {
        let now = Instant::now();
        let mut attempts = self.init_attempts.write().await;
        let entry = attempts.entry(source_ip.to_string()).or_default();
        entry.retain(|instant| now.duration_since(*instant) < INIT_RATE_WINDOW);
        if entry.len() >= INIT_RATE_LIMIT {
            return false;
        }
        entry.push(now);
        true
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
        self.store.get_by_phone_id(phone_id)
    }

    /// Resolve an already-authenticated opaque credential ID to its current
    /// pairing record. Revoked and expired records are never returned.
    pub fn find_by_pairing_id(&self, pairing_id: &str) -> Result<Option<PairedDeviceRecord>> {
        Ok(self
            .store
            .list_paired()?
            .into_iter()
            .find(|record| record.pairing_id == pairing_id)
            .filter(|record| record.session_token_expiry >= Utc::now()))
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
        if record.session_token_expiry < Utc::now() {
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

fn rejected_verify(reason: &str) -> PairVerifyResponse {
    PairVerifyResponse {
        status: "rejected".to_string(),
        server_signed_nonce: None,
        session_token: None,
        pairing_id: None,
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
        && (bind.starts_with("0.0.0.0:") || bind.starts_with("[::]:") || truthy_env("MEDOUSA_PAIRING_ADVERTISE"))
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
    ) -> (String, String, String) {
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
                "127.0.0.1",
            )
            .await
            .expect("init");
        assert_eq!(init.status, "challenge");
        let signed_nonce =
            sign_message(&phone, init.server_nonce.as_deref().expect("server nonce"));
        let mut phone_nonce = [0u8; 32];
        OsRng.fill_bytes(&mut phone_nonce);
        let verify = service
            .pair_verify(PairVerifyRequest {
                session_id: init.session_id.expect("session"),
                signed_nonce,
                phone_nonce: base64url_encode(&phone_nonce),
            })
            .await
            .expect("verify");
        assert_eq!(verify.status, "paired");
        (
            verify.pairing_id.expect("pairing"),
            verify.session_token.expect("session token"),
            phone_id,
        )
    }

    #[tokio::test]
    async fn qr_url_contains_device_id() {
        let service = test_service();
        let qr = service.current_qr().await.expect("qr");
        assert!(qr.url.contains("medousa://pair/1.0"));
        assert!(qr.url.contains(&service.device_id().to_string()));
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
        let first = service.pair_init(init.clone(), "127.0.0.1").await.expect("init");
        assert_eq!(first.status, "challenge");
        let second = service.pair_init(init, "127.0.0.1").await.expect("init");
        assert_eq!(second.status, "rejected");
        assert_eq!(second.reason.as_deref(), Some("token_already_used"));
    }

    #[tokio::test]
    async fn full_pairing_handshake() {
        let service = test_service();
        let (pairing_id, session_token, phone_id) = pair_test_phone(&service, None).await;
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
        let (pairing_id, session_token, phone_id) = pair_test_phone(&service, None).await;
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
        assert!(service.find_by_pairing_id(&pairing_id).unwrap().is_none());
        service.store.delete_record(&phone_id).expect("cleanup");
    }

    #[tokio::test]
    async fn revoke_requires_matching_credential_authority() {
        let service = test_service();
        let (pairing_id, session_token, _) = pair_test_phone(&service, Some("portal")).await;

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
                .revoke_pairing(
                    &pairing_id,
                    RevokePairingAuthority::Credential(&pairing_id),
                )
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
    }

    fn extract_query_param(url: &str, key: &str) -> Option<String> {
        let query = url.split('?').nth(1)?;
        for pair in query.split('&') {
            let (name, value) = pair.split_once('=')?;
            if name == key {
                return Some(value.to_string());
            }
        }
        None
    }
}
