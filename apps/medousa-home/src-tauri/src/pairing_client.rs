use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use rand::RngCore;
use rand::rngs::OsRng;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SESSION_TOKEN_SERVICE: &str = "medousa.pairing";
const LEGACY_SESSION_TOKEN_ACCOUNT: &str = "session_token";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingCredentialsSummary {
    pub pairing_id: String,
    pub phone_id: String,
    pub workshop_device_id: String,
    pub daemon_url: String,
    pub paired_at: String,
    pub has_session_token: bool,
    pub iroh_available: bool,
}

#[derive(Debug, Clone)]
pub struct WorkshopTransportConfig {
    pub lan_base: String,
    pub iroh_ticket: Option<String>,
    pub session_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairingCredentialsFile {
    pairing_id: String,
    phone_id: String,
    workshop_device_id: String,
    daemon_url: String,
    paired_at: String,
    #[serde(default)]
    iroh_ticket: Option<String>,
    #[serde(default)]
    workshop_endpoint_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairCompleteFromQrRequest {
    pub qr_url: String,
    pub daemon_url: String,
    #[serde(default)]
    pub phone_name: Option<String>,
    /// `portal` (full client) or `peer` (inbox/share only). Defaults to portal.
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairCompleteFromQrResult {
    pub pairing_id: String,
    pub phone_id: String,
    pub workshop_device_id: String,
    pub workshop_id: String,
    pub workshop_peer_name: String,
    pub daemon_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairStatusPayload {
    device_id: String,
    peer_name: String,
    daemon_public_key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairInitPayload {
    status: String,
    server_nonce: Option<String>,
    session_id: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairVerifyPayload {
    status: String,
    server_signed_nonce: Option<String>,
    session_token: Option<String>,
    pairing_id: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedQr {
    advertise_address: String,
    device_id: String,
    qr_token: String,
    signature: String,
    peer_name: String,
    iroh_ticket: Option<String>,
}

struct PhoneIdentity {
    phone_id: String,
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

pub async fn pair_complete_from_qr(
    request: PairCompleteFromQrRequest,
) -> Result<PairCompleteFromQrResult, String> {
    let parsed_qr = parse_pair_qr_url(&request.qr_url)?;
    let daemon_url = normalize_daemon_url(&request.daemon_url)?;
    let role = crate::workshop_registry::normalize_connection_role(
        request.role.as_deref().unwrap_or("portal"),
    );
    let phone_name = request
        .phone_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(if role == "peer" {
            "Medousa Peer"
        } else {
            "Medousa Phone"
        })
        .to_string();

    let client = http_client()?;
    let status = fetch_pair_status(&client, &daemon_url).await?;
    verify_qr_trust(&parsed_qr, &status)?;

    let identity = PhoneIdentity::load_or_create()?;
    let init = post_pair_init(
        &client,
        &daemon_url,
        &parsed_qr.qr_token,
        &identity.phone_id,
        &phone_name,
        &base64url_encode(identity.verifying_key.as_bytes()),
        role,
    )
    .await?;

    let server_nonce = init
        .server_nonce
        .as_deref()
        .ok_or_else(|| init_failure_message(init.reason.as_deref()))?;
    let session_id = init
        .session_id
        .as_deref()
        .ok_or_else(|| init_failure_message(init.reason.as_deref()))?;

    let signed_nonce = sign_message(&identity.signing_key, server_nonce);
    let mut phone_nonce = [0u8; 32];
    OsRng.fill_bytes(&mut phone_nonce);
    let phone_nonce_b64 = base64url_encode(&phone_nonce);

    let verify = post_pair_verify(
        &client,
        &daemon_url,
        session_id,
        &signed_nonce,
        &phone_nonce_b64,
    )
    .await?;

    if verify.status != "paired" {
        return Err(verify_failure_message(verify.reason.as_deref()));
    }

    let server_signed_nonce = verify
        .server_signed_nonce
        .as_deref()
        .ok_or_else(|| "Pairing verify succeeded but workshop did not return a signature".to_string())?;
    let daemon_public_key = parse_verifying_key(&status.daemon_public_key)?;
    verify_message(
        &daemon_public_key,
        &phone_nonce_b64,
        server_signed_nonce,
    )
    .map_err(|err| format!("Workshop signature check failed: {err}"))?;

    let pairing_id = verify
        .pairing_id
        .ok_or_else(|| "Pairing verify succeeded but pairing id was missing".to_string())?;
    let session_token = verify
        .session_token
        .ok_or_else(|| "Pairing verify succeeded but session token was missing".to_string())?;

    let iroh_ticket = if let Some(ticket) = parsed_qr.iroh_ticket.clone() {
        Some(ticket)
    } else {
        fetch_iroh_ticket(&client, &daemon_url).await.ok()
    };

    let workshop_id = crate::workshop_registry::register_paired_workshop(
        crate::workshop_registry::RegisterPairedInput {
            pairing_id: pairing_id.clone(),
            phone_id: identity.phone_id.clone(),
            workshop_device_id: status.device_id.clone(),
            workshop_peer_name: status.peer_name.clone(),
            daemon_url: daemon_url.clone(),
            paired_at: chrono::Utc::now().to_rfc3339(),
            has_iroh_ticket: iroh_ticket.is_some(),
            role: role.to_string(),
        },
    )?;

    save_pairing_credentials(
        &workshop_id,
        &pairing_id,
        &identity.phone_id,
        &status.device_id,
        &daemon_url,
        &session_token,
        iroh_ticket.as_deref(),
    )?;

    crate::workshop_transport::invalidate_workshop_route_cache();

    Ok(PairCompleteFromQrResult {
        pairing_id,
        phone_id: identity.phone_id,
        workshop_device_id: status.device_id,
        workshop_id,
        workshop_peer_name: status.peer_name,
        daemon_url,
    })
}

pub fn load_pairing_credentials_summary() -> Option<PairingCredentialsSummary> {
    let file = read_credentials_file()?;
    let workshop_device_id = file.workshop_device_id.clone();
    let has_session_token = read_session_token(&workshop_device_id).is_some();
    Some(PairingCredentialsSummary {
        pairing_id: file.pairing_id,
        phone_id: file.phone_id,
        workshop_device_id,
        daemon_url: file.daemon_url,
        paired_at: file.paired_at,
        has_session_token,
        iroh_available: file.iroh_ticket.as_ref().is_some_and(|t| !t.trim().is_empty()),
    })
}

pub fn load_workshop_transport_config(lan_base: &str) -> Option<WorkshopTransportConfig> {
    let file = read_credentials_file()?;
    build_transport_config(&file, lan_base)
}

pub fn load_workshop_transport_config_for_id(
    workshop_id: &str,
    lan_base: &str,
) -> Option<WorkshopTransportConfig> {
    let path = crate::workshop_registry::pairing_credentials_abs_path(workshop_id);
    let raw = fs::read_to_string(path).ok()?;
    let file = serde_json::from_str::<PairingCredentialsFile>(&raw).ok()?;
    build_transport_config(&file, lan_base)
}

pub fn workshop_has_session_token(workshop_id: &str, workshop_device_id: &str) -> bool {
    read_session_token(workshop_device_id).is_some()
        || read_credentials_for_workshop(workshop_id)
            .is_some_and(|file| read_session_token(&file.workshop_device_id).is_some())
}

fn build_transport_config(file: &PairingCredentialsFile, lan_base: &str) -> Option<WorkshopTransportConfig> {
    let lan = lan_base.trim().trim_end_matches('/').to_string();
    Some(WorkshopTransportConfig {
        lan_base: if lan.is_empty() {
            file.daemon_url.clone()
        } else {
            lan
        },
        iroh_ticket: file
            .iroh_ticket
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        session_token: read_session_token(&file.workshop_device_id),
    })
}

fn read_credentials_for_workshop(workshop_id: &str) -> Option<PairingCredentialsFile> {
    let path = crate::workshop_registry::pairing_credentials_abs_path(workshop_id);
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub async fn send_pair_heartbeat(
    lan_base: &str,
    body: Option<&crate::pairing::PairHeartbeatInvokeRequest>,
) -> Result<(), String> {
    let Some(config) = load_workshop_transport_config(lan_base) else {
        return Ok(());
    };

    let apns_token = body
        .and_then(|body| body.apns_device_token.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(crate::push::current_apns_device_token);

    let live_activity_token = match body.and_then(|body| body.live_activity_push_token.as_ref()) {
        Some(value) => Some(value.trim().to_string()),
        None => {
            #[cfg(target_os = "ios")]
            {
                crate::live_activity::current_push_token()
            }
            #[cfg(not(target_os = "ios"))]
            {
                None
            }
        }
    };

    if apns_token.is_some()
        || body.and_then(|body| body.push_platform.as_deref()).is_some()
        || live_activity_token.is_some()
    {
        let mut payload = serde_json::Map::new();
        if let Some(token) = apns_token {
            payload.insert("apnsDeviceToken".into(), serde_json::Value::String(token));
            payload.insert(
                "pushPlatform".into(),
                serde_json::Value::String(
                    body.and_then(|body| body.push_platform.as_deref())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .unwrap_or("ios")
                        .to_string(),
                ),
            );
        }
        if let Some(token) = live_activity_token {
            payload.insert(
                "liveActivityPushToken".into(),
                serde_json::Value::String(token),
            );
        }
        crate::workshop_transport::workshop_post_json::<serde_json::Value, _>(
            &config,
            "/pair/heartbeat",
            &serde_json::Value::Object(payload),
        )
        .await?;
        return Ok(());
    }

    crate::workshop_transport::workshop_get(&config, "/pair/heartbeat").await?;
    Ok(())
}

fn verify_qr_trust(parsed: &ParsedQr, status: &PairStatusPayload) -> Result<(), String> {
    if parsed.device_id != status.device_id {
        return Err(
            "Pairing link does not match this workshop — scan a fresh QR from your computer."
                .to_string(),
        );
    }
    let daemon_public_key = parse_verifying_key(&status.daemon_public_key)?;
    if let Some(iroh_ticket) = &parsed.iroh_ticket {
        verify_qr_url_signature_v2(
            &daemon_public_key,
            &parsed.advertise_address,
            &parsed.device_id,
            &parsed.qr_token,
            iroh_ticket,
            &parsed.signature,
        )
        .map_err(|err| format!("Pairing link signature invalid: {err}"))
    } else {
        verify_qr_url_signature(
            &daemon_public_key,
            &parsed.advertise_address,
            &parsed.device_id,
            &parsed.qr_token,
            &parsed.signature,
        )
        .map_err(|err| format!("Pairing link signature invalid: {err}"))
    }
}

async fn fetch_iroh_ticket(client: &Client, daemon_url: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TicketPayload {
        ticket: String,
        available: bool,
    }
    let response = client
        .get(format!("{daemon_url}/pair/iroh-ticket"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("iroh ticket unavailable (HTTP {})", response.status()));
    }
    let payload = response
        .json::<TicketPayload>()
        .await
        .map_err(|err| err.to_string())?;
    if !payload.available || payload.ticket.trim().is_empty() {
        return Err("workshop iroh ticket not available".to_string());
    }
    Ok(payload.ticket)
}

async fn fetch_pair_status(client: &Client, daemon_url: &str) -> Result<PairStatusPayload, String> {
    let response = client
        .get(format!("{daemon_url}/pair/status"))
        .send()
        .await
        .map_err(|err| format!("Cannot reach workshop at {daemon_url}: {err}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format_workshop_http_error(status.as_u16(), &body));
    }
    response
        .json::<PairStatusPayload>()
        .await
        .map_err(|err| err.to_string())
}

async fn post_pair_init(
    client: &Client,
    daemon_url: &str,
    qr_token: &str,
    phone_id: &str,
    phone_name: &str,
    public_key: &str,
    role: &str,
) -> Result<PairInitPayload, String> {
    let response = client
        .post(format!("{daemon_url}/pair/init"))
        .json(&serde_json::json!({
            "qrToken": qr_token,
            "phoneId": phone_id,
            "phoneName": phone_name,
            "publicKey": public_key,
            "role": role,
        }))
        .send()
        .await
        .map_err(|err| format!("Pair init failed: {err}"))?;

    let payload = response
        .json::<PairInitPayload>()
        .await
        .map_err(|err| err.to_string())?;
    if payload.status != "challenge" {
        return Err(init_failure_message(payload.reason.as_deref()));
    }
    Ok(payload)
}

async fn post_pair_verify(
    client: &Client,
    daemon_url: &str,
    session_id: &str,
    signed_nonce: &str,
    phone_nonce: &str,
) -> Result<PairVerifyPayload, String> {
    let response = client
        .post(format!("{daemon_url}/pair/verify"))
        .json(&serde_json::json!({
            "sessionId": session_id,
            "signedNonce": signed_nonce,
            "phoneNonce": phone_nonce,
        }))
        .send()
        .await
        .map_err(|err| format!("Pair verify failed: {err}"))?;

    response
        .json::<PairVerifyPayload>()
        .await
        .map_err(|err| err.to_string())
}

fn parse_pair_qr_url(raw: &str) -> Result<ParsedQr, String> {
    let trimmed = raw.trim();
    let url = Url::parse(trimmed).map_err(|_| "Pairing link must start with medousa://".to_string())?;
    if url.scheme() != "medousa" || url.host_str().unwrap_or_default() != "pair" {
        return Err("Not a Medousa pairing link".to_string());
    }

    let advertise_address = query_param(&url, "a").ok_or_else(|| "Pairing link is missing address".to_string())?;
    let device_id = query_param(&url, "d").ok_or_else(|| "Pairing link is missing device id".to_string())?;
    let qr_token = query_param(&url, "t").ok_or_else(|| "Pairing link is missing token".to_string())?;
    let signature = query_param(&url, "s").ok_or_else(|| "Pairing link is missing signature".to_string())?;
    let peer_name = query_param(&url, "n")
        .map(|value| urlencoding::decode(&value).map(|decoded| decoded.into_owned()).unwrap_or(value))
        .unwrap_or_else(|| "Medousa".to_string());
    let iroh_ticket = query_param(&url, "k");

    Ok(ParsedQr {
        advertise_address,
        device_id,
        qr_token,
        signature,
        peer_name,
        iroh_ticket,
    })
}

fn query_param(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.trim().is_empty())
}

fn normalize_daemon_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Workshop address is required".to_string());
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(trimmed.to_string());
    }
    Ok(format!("http://{trimmed}"))
}

impl PhoneIdentity {
    fn load_or_create() -> Result<Self, String> {
        let path = phone_identity_path();
        if path.is_file() {
            Self::load_from_path(&path)
        } else {
            Self::create_at_path(&path)
        }
    }

    fn load_from_path(path: &PathBuf) -> Result<Self, String> {
        let raw = fs::read_to_string(path).map_err(|err| err.to_string())?;
        let bytes = decode_secret_bytes(raw.trim())?;
        if bytes.len() != 32 {
            return Err("Phone identity secret must be 32 bytes".to_string());
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&bytes);
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();
        Ok(Self {
            phone_id: device_id_from_public_key(verifying_key.as_bytes()),
            signing_key,
            verifying_key,
        })
    }

    fn create_at_path(path: &PathBuf) -> Result<Self, String> {
        let signing_key = SigningKey::generate(&mut OsRng);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let encoded = encode_hex(signing_key.to_bytes().as_slice());
        fs::write(path, format!("{encoded}\n")).map_err(|err| err.to_string())?;
        let verifying_key = signing_key.verifying_key();
        Ok(Self {
            phone_id: device_id_from_public_key(verifying_key.as_bytes()),
            signing_key,
            verifying_key,
        })
    }
}

fn save_pairing_credentials(
    workshop_id: &str,
    pairing_id: &str,
    phone_id: &str,
    workshop_device_id: &str,
    daemon_url: &str,
    session_token: &str,
    iroh_ticket: Option<&str>,
) -> Result<(), String> {
    store_session_token(session_token, workshop_device_id)?;
    let path = crate::workshop_registry::pairing_credentials_abs_path(workshop_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let file = PairingCredentialsFile {
        pairing_id: pairing_id.to_string(),
        phone_id: phone_id.to_string(),
        workshop_device_id: workshop_device_id.to_string(),
        daemon_url: daemon_url.to_string(),
        paired_at: chrono::Utc::now().to_rfc3339(),
        iroh_ticket: iroh_ticket
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        workshop_endpoint_id: None,
    };
    let body = serde_json::to_string_pretty(&file).map_err(|err| err.to_string())?;
    fs::write(path, body).map_err(|err| err.to_string())
}

fn read_credentials_file() -> Option<PairingCredentialsFile> {
    let registry = crate::workshop_registry::ensure_migrated().ok()?;
    let workshop = crate::workshop_registry::active_workshop(&registry)?;
    if workshop.kind != "paired" {
        return None;
    }
    let rel = workshop
        .pairing
        .as_ref()
        .and_then(|pairing| pairing.credentials_rel_path.as_deref())?;
    let path = crate::workshop_registry::medousa_data_dir().join(rel);
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn migrate_legacy_session_token(workshop_device_id: &str) {
    if read_session_token(workshop_device_id).is_some() {
        return;
    }
    let Some(legacy) = read_legacy_session_token() else {
        return;
    };
    let _ = store_session_token(&legacy, workshop_device_id);
}

fn session_token_account(workshop_device_id: &str) -> String {
    format!("session_token.{workshop_device_id}")
}

fn store_session_token(token: &str, workshop_device_id: &str) -> Result<(), String> {
    let account = session_token_account(workshop_device_id);
    if let Ok(entry) = keyring::Entry::new(SESSION_TOKEN_SERVICE, &account) {
        if entry.set_password(token).is_ok() {
            return Ok(());
        }
    }
    let path = session_token_file_path(workshop_device_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(path, token).map_err(|err| err.to_string())
}

fn delete_session_token(workshop_device_id: &str) {
    let account = session_token_account(workshop_device_id);
    if let Ok(entry) = keyring::Entry::new(SESSION_TOKEN_SERVICE, &account) {
        let _ = entry.delete_password();
    }
    let _ = fs::remove_file(session_token_file_path(workshop_device_id));
}

pub fn remove_workshop_credentials(
    workshop_id: &str,
    workshop_device_id: Option<&str>,
) -> Result<(), String> {
    let path = crate::workshop_registry::pairing_credentials_abs_path(workshop_id);
    let device_id = workshop_device_id
        .map(str::to_string)
        .or_else(|| {
            fs::read_to_string(&path)
                .ok()
                .and_then(|raw| serde_json::from_str::<PairingCredentialsFile>(&raw).ok())
                .map(|file| file.workshop_device_id)
        });
    if path.exists() {
        fs::remove_file(&path).map_err(|err| err.to_string())?;
    }
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir(parent);
    }
    if let Some(device_id) = device_id {
        delete_session_token(&device_id);
    }
    Ok(())
}

fn read_session_token(workshop_device_id: &str) -> Option<String> {
    let account = session_token_account(workshop_device_id);
    if let Ok(entry) = keyring::Entry::new(SESSION_TOKEN_SERVICE, &account) {
        if let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    let value = fs::read_to_string(session_token_file_path(workshop_device_id)).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn read_legacy_session_token() -> Option<String> {
    if let Ok(entry) = keyring::Entry::new(SESSION_TOKEN_SERVICE, LEGACY_SESSION_TOKEN_ACCOUNT) {
        if let Ok(value) = entry.get_password() {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    let legacy_path = medousa_data_dir().join("secrets").join("pairing_session_token");
    let value = fs::read_to_string(legacy_path).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn medousa_data_dir() -> PathBuf {
    crate::paths::medousa_data_dir()
}

fn phone_identity_path() -> PathBuf {
    medousa_data_dir().join("phone_identity.secret")
}

fn pairing_credentials_path() -> PathBuf {
    medousa_data_dir().join("pairing_credentials.json")
}

fn session_token_file_path(workshop_device_id: &str) -> PathBuf {
    medousa_data_dir()
        .join("secrets")
        .join(format!("pairing_session_token.{workshop_device_id}"))
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|err| err.to_string())
}

fn init_failure_message(reason: Option<&str>) -> String {
    match reason.unwrap_or("unknown") {
        "rate_limited" => "Too many pairing attempts — wait a minute and try again.".to_string(),
        "missing_token" => "Pairing link is missing a token — scan a fresh QR.".to_string(),
        "no_active_qr" => "No active QR on your computer — open Pair phone and scan again.".to_string(),
        "token_already_used" => "This pairing link was already used — scan a fresh QR.".to_string(),
        "token_expired" => "This pairing link expired — scan a fresh QR.".to_string(),
        "invalid_token" => "This pairing link no longer matches your computer — scan a fresh QR.".to_string(),
        other => format!("Pairing was rejected ({other})"),
    }
}

fn verify_failure_message(reason: Option<&str>) -> String {
    match reason.unwrap_or("unknown") {
        "unknown_session" => "Pairing session expired — scan a fresh QR and try again.".to_string(),
        "verify_timeout" => "Pairing timed out — scan a fresh QR and try again.".to_string(),
        other => format!("Pairing verify failed ({other})"),
    }
}

fn format_workshop_http_error(status: u16, body: &str) -> String {
    if body.trim().is_empty() {
        format!("Workshop returned HTTP {status}")
    } else {
        format!("Workshop returned HTTP {status}: {}", body.trim())
    }
}

fn device_id_from_public_key(public_key: &[u8; 32]) -> String {
    let digest = Sha256::digest(public_key);
    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn base64url_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn base64url_decode(value: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value.trim())
        .map_err(|err| err.to_string())
}

fn sign_message(signing_key: &SigningKey, message: &str) -> String {
    let signature = signing_key.sign(message.as_bytes());
    base64url_encode(signature.to_bytes().as_slice())
}

fn verify_message(
    verifying_key: &VerifyingKey,
    message: &str,
    signature_b64: &str,
) -> Result<(), String> {
    let bytes = base64url_decode(signature_b64)?;
    let signature = ed25519_dalek::Signature::from_slice(&bytes)
        .map_err(|_| "Invalid workshop signature".to_string())?;
    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|err| err.to_string())
}

fn verify_qr_url_signature(
    verifying_key: &VerifyingKey,
    address: &str,
    device_id: &str,
    token_b64: &str,
    signature_b64: &str,
) -> Result<(), String> {
    let message = format!("{address}|{device_id}|{token_b64}");
    verify_message(verifying_key, &message, signature_b64)
}

fn verify_qr_url_signature_v2(
    verifying_key: &VerifyingKey,
    address: &str,
    device_id: &str,
    token_b64: &str,
    iroh_ticket: &str,
    signature_b64: &str,
) -> Result<(), String> {
    let message = format!("{address}|{device_id}|{token_b64}|{iroh_ticket}");
    verify_message(verifying_key, &message, signature_b64)
}

fn parse_verifying_key(public_key_b64: &str) -> Result<VerifyingKey, String> {
    let bytes = base64url_decode(public_key_b64)?;
    if bytes.len() != 32 {
        return Err("Workshop public key must be 32 bytes".to_string());
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    VerifyingKey::from_bytes(&key).map_err(|err| err.to_string())
}

fn decode_secret_bytes(raw: &str) -> Result<Vec<u8>, String> {
    if raw.len() == 64 && raw.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return (0..raw.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&raw[index..index + 2], 16))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| err.to_string());
    }
    base64url_decode(raw)
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pair_qr_url() {
        let parsed = parse_pair_qr_url(
            "medousa://pair/1.0?a=192.168.1.2:7419&d=abcd1234&t=token&s=sig&n=Desk",
        )
        .expect("parse");
        assert_eq!(parsed.advertise_address, "192.168.1.2:7419");
        assert_eq!(parsed.device_id, "abcd1234");
        assert_eq!(parsed.qr_token, "token");
        assert_eq!(parsed.signature, "sig");
        assert_eq!(parsed.peer_name, "Desk");
    }
}
