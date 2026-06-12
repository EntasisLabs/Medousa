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
    PROTOCOL_VERSION, QR_SCHEME, base64url_decode, base64url_encode, hash_session_token,
    parse_verifying_key, qr_signing_message, sign_message, verify_message, verifying_key_to_b64,
};
use super::identity::DeviceIdentity;
use super::store::{PairedDeviceRecord, PairingStore};

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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairedDeviceSummary {
    pub pairing_id: String,
    pub phone_id: String,
    pub phone_name: String,
    pub paired_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairInitRequest {
    pub qr_token: Option<String>,
    pub short_code: Option<String>,
    pub phone_id: String,
    pub phone_name: String,
    pub public_key: String,
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

#[derive(Debug, Clone)]
struct ActiveQrSession {
    token_b64: String,
    short_code: String,
    short_code_raw: String,
    expires_at: DateTime<Utc>,
    used: bool,
}

#[derive(Debug, Clone)]
struct PendingPairSession {
    phone_id: String,
    phone_name: String,
    phone_public_key: VerifyingKey,
    server_nonce: [u8; 32],
    created_at: Instant,
}

pub struct PairingService {
    identity: DeviceIdentity,
    store: PairingStore,
    advertise_address: String,
    peer_name: String,
    model_descriptor: Option<String>,
    auth_required: bool,
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
    ) -> Self {
        let store = PairingStore::new(identity.signing_key());
        Self {
            identity,
            store,
            advertise_address,
            peer_name,
            model_descriptor,
            auth_required: true,
            active_qr: RwLock::new(None),
            pending_sessions: RwLock::new(HashMap::new()),
            init_attempts: RwLock::new(HashMap::new()),
        }
    }

    pub fn device_id(&self) -> &str {
        &self.identity.device_id
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
        // pairing_v1 + web_transcript + voice_push + file_push + brain_sync
        "001F".to_string()
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
                })
                .collect(),
            qr_active,
            device_id: self.identity.device_id.clone(),
            peer_name: self.peer_name.clone(),
            protocol_version: PROTOCOL_VERSION.to_string(),
        })
    }

    pub async fn current_qr(&self) -> Result<QrResponse> {
        let mut guard = self.active_qr.write().await;
        let needs_refresh = guard.as_ref().is_none_or(|session| {
            session.used || session.expires_at <= Utc::now()
        });
        if needs_refresh {
            *guard = Some(self.build_qr_session()?);
        }
        let session = guard.as_ref().expect("qr session");
        Ok(QrResponse {
            url: self.build_qr_url(session)?,
            expires_at: session.expires_at,
            short_code: session.short_code.clone(),
        })
    }

    pub async fn current_short_code(&self) -> Result<String> {
        Ok(self.current_qr().await?.short_code)
    }

    pub async fn current_qr_image(&self) -> Result<QrImageResponse> {
        let qr = self.current_qr().await?;
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

        let mut server_nonce = [0u8; 32];
        OsRng.fill_bytes(&mut server_nonce);
        let session_id = Uuid::new_v4();
        self.pending_sessions.write().await.insert(
            session_id,
            PendingPairSession {
                phone_id: request.phone_id.clone(),
                phone_name: request.phone_name.clone(),
                phone_public_key: phone_key,
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
        };
        self.store.save_record(&record)?;

        Ok(PairVerifyResponse {
            status: "paired".to_string(),
            server_signed_nonce: Some(server_signed_nonce),
            session_token: Some(session_token),
            pairing_id: Some(pairing_id),
            reason: None,
        })
    }

    pub async fn pair_heartbeat(&self, bearer_token: Option<&str>) -> Result<PairHeartbeatResponse> {
        let Some(token) = bearer_token else {
            return Ok(PairHeartbeatResponse {
                status: "unauthorized".to_string(),
                device_time: Utc::now(),
                reason: Some("missing_token".to_string()),
            });
        };
        let record = self.find_by_session_token(token)?;
        let Some(record) = record else {
            return Ok(PairHeartbeatResponse {
                status: "unauthorized".to_string(),
                device_time: Utc::now(),
                reason: Some("invalid_token".to_string()),
            });
        };
        if self.store.is_revoked(&record.pairing_id)? {
            return Ok(PairHeartbeatResponse {
                status: "unauthorized".to_string(),
                device_time: Utc::now(),
                reason: Some("revoked".to_string()),
            });
        }
        if record.session_token_expiry < Utc::now() {
            return Ok(PairHeartbeatResponse {
                status: "unauthorized".to_string(),
                device_time: Utc::now(),
                reason: Some("expired".to_string()),
            });
        }
        self.store.touch_last_seen(&record.phone_id)?;
        Ok(PairHeartbeatResponse {
            status: "ok".to_string(),
            device_time: Utc::now(),
            reason: None,
        })
    }

    pub async fn revoke_pairing(&self, pairing_id: &str) -> Result<bool> {
        let paired = self.store.list_paired()?;
        let Some(record) = paired.into_iter().find(|entry| entry.pairing_id == pairing_id) else {
            return Ok(false);
        };
        self.store.revoke_pairing(pairing_id)?;
        self.store.delete_record(&record.phone_id)?;
        Ok(true)
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

    fn build_qr_session(&self) -> Result<ActiveQrSession> {
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
        })
    }

    fn build_qr_url(&self, session: &ActiveQrSession) -> Result<String> {
        let message = qr_signing_message(
            &self.advertise_address,
            &self.identity.device_id,
            &session.token_b64,
        );
        let signature = sign_message(self.identity.signing_key(), &message);
        let name = urlencoding::encode(&self.peer_name);
        Ok(format!(
            "{QR_SCHEME}?a={}&d={}&t={}&s={}&n={name}",
            urlencoding::encode(&self.advertise_address),
            self.identity.device_id,
            session.token_b64,
            signature,
        ))
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

    fn find_by_session_token(&self, token: &str) -> Result<Option<PairedDeviceRecord>> {
        let hash = hash_session_token(token);
        for record in self.store.list_paired()? {
            if record.session_token_hash == hash {
                return Ok(Some(record));
            }
        }
        Ok(None)
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
    for index in 0..6 {
        let byte = digest[index];
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
        ))
    }

    #[tokio::test]
    async fn qr_url_contains_device_id() {
        let service = test_service();
        let qr = service.current_qr().await.expect("qr");
        assert!(qr.url.contains("medousa://pair/1.0"));
        assert!(qr.url.contains(&service.device_id().to_string()));
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
        let qr = service.current_qr().await.expect("qr");
        let token = extract_query_param(&qr.url, "t").expect("token");
        let phone = SigningKey::generate(&mut OsRng);
        let init = service
            .pair_init(
                PairInitRequest {
                    qr_token: Some(token),
                    short_code: None,
                    phone_id: "phone0002".to_string(),
                    phone_name: "Phone B".to_string(),
                    public_key: verifying_key_to_b64(&phone.verifying_key()),
                },
                "127.0.0.1",
            )
            .await
            .expect("init");
        assert_eq!(init.status, "challenge");
        let session_id = init.session_id.expect("session");
        let server_nonce = init.server_nonce.expect("nonce");
        let signed_nonce = sign_message(&phone, &server_nonce);
        let mut phone_nonce = [0u8; 32];
        OsRng.fill_bytes(&mut phone_nonce);
        let verify = service
            .pair_verify(PairVerifyRequest {
                session_id,
                signed_nonce,
                phone_nonce: base64url_encode(&phone_nonce),
            })
            .await
            .expect("verify");
        assert_eq!(verify.status, "paired");
        assert!(verify.session_token.is_some());
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
