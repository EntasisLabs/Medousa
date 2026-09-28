//! Browser portal: pair with a full daemon over an Iroh invite and send its HTTP API.
//!
//! The tab keeps a Personal workshop in IndexedDB. A `medousa://pair/2.0` invite
//! adds a separate portal. Chat and daemon calls for that workshop go through
//! `medousa-http/1`, the same relay client the phone uses.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::browser_portal_parse::{SseParser, query_param};
use crate::pairing_crypto::{
    self, base64url_encode, device_id_from_public_key, parse_verifying_key, sign_message,
    verify_message, verify_qr_url_signature_v2_with_profile,
};

const DEVICE_SECRET_KEY: &str = "medousa.browser.device_secret";
const PORTAL_STORE_KEY: &str = "medousa.browser.portals";
const SESSION_REFRESH_SKEW_SECONDS: i64 = 5 * 60;
const PERSONAL_WORKSHOP_ID: &str = "personal";

static REFRESH_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static STREAM_CANCEL: Mutex<Option<HashMap<String, watch::Sender<bool>>>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PortalRecord {
    workshop_id: String,
    pairing_id: String,
    phone_id: String,
    workshop_device_id: String,
    peer_name: String,
    daemon_url: String,
    paired_at: String,
    #[serde(default)]
    session_expires_at: Option<String>,
    iroh_ticket: String,
    daemon_public_key: String,
    session_token: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PortalStore {
    #[serde(default)]
    active_workshop_id: String,
    #[serde(default)]
    portals: Vec<PortalRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairStatusPayload {
    device_id: String,
    peer_name: String,
    daemon_public_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairInitPayload {
    status: String,
    server_nonce: Option<String>,
    session_id: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairVerifyPayload {
    status: String,
    server_signed_nonce: Option<String>,
    session_token: Option<String>,
    pairing_id: Option<String>,
    session_expires_at: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairSessionChallengePayload {
    status: String,
    session_id: Option<String>,
    server_nonce: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairSessionRefreshPayload {
    status: String,
    server_signed_nonce: Option<String>,
    session_token: Option<String>,
    session_expires_at: Option<String>,
    reason: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PairResult {
    pairing_id: String,
    phone_id: String,
    workshop_device_id: String,
    workshop_id: String,
    workshop_peer_name: String,
    daemon_url: String,
}

struct DeviceIdentity {
    phone_id: String,
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

struct ParsedInvite {
    advertise_address: String,
    device_id: String,
    qr_token: String,
    signature: String,
    peer_name: String,
    daemon_public_key: Option<String>,
    iroh_ticket: String,
    profile_id: Option<String>,
}

pub fn pair_result_json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|err| err.to_string())
}

/// Run the Ed25519 invite ceremony over the ticket inside a full v2 link.
pub async fn pair_from_invite(qr_url: &str, display_name: &str) -> Result<String, String> {
    let invite = parse_invite(qr_url)?;
    let status = match invite.daemon_public_key.clone() {
        Some(daemon_public_key) => PairStatusPayload {
            device_id: invite.device_id.clone(),
            peer_name: invite.peer_name.clone(),
            daemon_public_key,
        },
        None => {
            iroh_json(
                &invite.iroh_ticket,
                Some(invite.advertise_address.as_str()),
                "GET",
                "/pair/status",
                None,
                &[],
            )
            .await?
        }
    };
    if invite.device_id != status.device_id {
        return Err(
            "Pairing link does not match this workshop — paste a fresh invite.".to_string(),
        );
    }
    let daemon_key = parse_verifying_key(&status.daemon_public_key)
        .map_err(|err| format!("Workshop public key is invalid: {err}"))?;
    verify_qr_url_signature_v2_with_profile(
        &daemon_key,
        &invite.advertise_address,
        &invite.device_id,
        &invite.qr_token,
        &invite.iroh_ticket,
        invite.profile_id.as_deref(),
        &invite.signature,
    )
    .map_err(|err| format!("Pairing link signature invalid: {err}"))?;

    let identity = load_or_create_identity()?;
    let phone_name = display_name.trim();
    let phone_name = if phone_name.is_empty() {
        "Medousa"
    } else {
        phone_name
    };
    let init_body = serde_json::json!({
        "qrToken": invite.qr_token,
        "phoneId": identity.phone_id,
        "phoneName": phone_name,
        "publicKey": pairing_crypto::verifying_key_to_b64(&identity.verifying_key),
        "role": "portal",
    });
    let init: PairInitPayload = iroh_json(
        &invite.iroh_ticket,
        Some(invite.advertise_address.as_str()),
        "POST",
        "/pair/init",
        Some(&init_body),
        &[],
    )
    .await?;
    if init.status != "challenge" {
        return Err(init_failure_message(init.reason.as_deref()));
    }
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
    let verify_body = serde_json::json!({
        "sessionId": session_id,
        "signedNonce": signed_nonce,
        "phoneNonce": phone_nonce_b64,
    });
    let verify: PairVerifyPayload = iroh_json(
        &invite.iroh_ticket,
        Some(invite.advertise_address.as_str()),
        "POST",
        "/pair/verify",
        Some(&verify_body),
        &[],
    )
    .await?;
    if verify.status != "paired" {
        return Err(verify_failure_message(verify.reason.as_deref()));
    }
    let server_signed_nonce = verify.server_signed_nonce.as_deref().ok_or_else(|| {
        "Pairing verify succeeded but the workshop did not return a signature".to_string()
    })?;
    verify_message(&daemon_key, &phone_nonce_b64, server_signed_nonce)
        .map_err(|err| format!("Workshop signature check failed: {err}"))?;
    let pairing_id = verify
        .pairing_id
        .ok_or_else(|| "Pairing verify succeeded but the pairing id was missing".to_string())?;
    let session_token = verify
        .session_token
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Pairing verify succeeded but the session token was missing".to_string())?;

    let workshop_id = format!("paired-{}", status.device_id);
    let daemon_url = normalize_daemon_url(&invite.advertise_address);
    let record = PortalRecord {
        workshop_id: workshop_id.clone(),
        pairing_id,
        phone_id: identity.phone_id.clone(),
        workshop_device_id: status.device_id.clone(),
        peer_name: status.peer_name.clone(),
        daemon_url: daemon_url.clone(),
        paired_at: chrono::Utc::now().to_rfc3339(),
        session_expires_at: verify.session_expires_at,
        iroh_ticket: invite.iroh_ticket,
        daemon_public_key: status.daemon_public_key,
        session_token,
    };
    let mut store = read_store();
    store
        .portals
        .retain(|portal| portal.workshop_id != workshop_id);
    store.portals.push(record.clone());
    write_store(&store)?;

    pair_result_json(&PairResult {
        pairing_id: record.pairing_id,
        phone_id: record.phone_id,
        workshop_device_id: record.workshop_device_id,
        workshop_id,
        workshop_peer_name: record.peer_name,
        daemon_url,
    })
}

pub fn set_active_portal(workshop_id: &str) -> Result<(), String> {
    let workshop_id = workshop_id.trim();
    let mut store = read_store();
    if workshop_id.is_empty() || workshop_id == PERSONAL_WORKSHOP_ID {
        store.active_workshop_id.clear();
        return write_store(&store);
    }
    if !store
        .portals
        .iter()
        .any(|portal| portal.workshop_id == workshop_id)
    {
        return Err("That workshop is not paired in this browser.".to_string());
    }
    store.active_workshop_id = workshop_id.to_string();
    write_store(&store)
}

pub fn forget_portal(workshop_id: &str) -> Result<(), String> {
    let mut store = read_store();
    store
        .portals
        .retain(|portal| portal.workshop_id != workshop_id);
    if store.active_workshop_id == workshop_id {
        store.active_workshop_id.clear();
    }
    write_store(&store)
}

/// Authenticated request to the active portal. A 401 refreshes the session once.
pub async fn portal_request(method: &str, path: &str, body: &str) -> Result<String, String> {
    let mut record = active_record()?;
    record = ensure_fresh_session(record).await?;
    let encoded = if body.is_empty() {
        None
    } else {
        Some(body.as_bytes().to_vec())
    };
    let (status, response_body) =
        authed_exchange(&record, method, path, encoded.as_deref(), &[]).await?;
    if status == 401 {
        record = refresh_session(record).await?;
        let (status, response_body) =
            authed_exchange(&record, method, path, encoded.as_deref(), &[]).await?;
        return exchange_json(status, &response_body);
    }
    exchange_json(status, &response_body)
}

/// Read an SSE response, invoking `on_event` with each JSON `data:` payload.
pub async fn open_stream(
    kind: &str,
    path: &str,
    accept: &str,
    on_event: impl Fn(String),
) -> Result<(), String> {
    let mut record = active_record()?;
    record = ensure_fresh_session(record).await?;
    let cancel = arm_stream(kind);
    let mut response = open_stream_response(&record, path, accept).await;
    if response.as_ref().is_ok_and(|value| value.0 == 401) {
        record = refresh_session(record).await?;
        response = open_stream_response(&record, path, accept).await;
    }
    let (status, mut body) = response?;
    if !(200..300).contains(&status) {
        let message = read_body_text(&mut body).await?;
        return Err(format!(
            "workshop returned HTTP {status} over iroh for GET {path}: {message}"
        ));
    }
    let mut parser = SseParser::default();
    loop {
        if *cancel.borrow() {
            return Ok(());
        }
        let mut cancel_wait = cancel.clone();
        tokio::select! {
            changed = cancel_wait.changed() => {
                if changed.is_err() || *cancel.borrow() {
                    return Ok(());
                }
            }
            chunk = body.read_chunk() => {
                match chunk.map_err(|err| err.to_string())? {
                    None => return Ok(()),
                    Some(bytes) => {
                        for frame in parser.push(&bytes) {
                            on_event(frame);
                        }
                    }
                }
            }
        }
    }
}

pub fn stop_streams(prefix: &str) {
    let mut guard = STREAM_CANCEL.lock().expect("stream cancel lock");
    let Some(map) = guard.as_mut() else {
        return;
    };
    let keys: Vec<String> = map
        .keys()
        .filter(|key| key.starts_with(prefix))
        .cloned()
        .collect();
    for key in keys {
        if let Some(sender) = map.remove(&key) {
            let _ = sender.send(true);
        }
    }
}

fn arm_stream(kind: &str) -> watch::Receiver<bool> {
    let (sender, receiver) = watch::channel(false);
    let mut guard = STREAM_CANCEL.lock().expect("stream cancel lock");
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(previous) = map.insert(kind.to_string(), sender) {
        let _ = previous.send(true);
    }
    receiver
}

async fn open_stream_response(
    record: &PortalRecord,
    path: &str,
    accept: &str,
) -> Result<(u16, medousa_iroh_http::IrohHttpBody), String> {
    let accept = accept.trim();
    let mut headers = vec![(
        "Authorization".to_string(),
        format!("Bearer {}", record.session_token),
    )];
    if !accept.is_empty() {
        headers.push(("Accept".to_string(), accept.to_string()));
    }
    let header_refs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let response =
        medousa_iroh_http::iroh_http_request(&record.iroh_ticket, "GET", path, &header_refs, None)
            .await
            .map_err(|err| format!("Cannot reach workshop over Iroh: {err}"))?;
    Ok((response.status, response.body))
}

async fn authed_exchange(
    record: &PortalRecord,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    extra_headers: &[(&str, &str)],
) -> Result<(u16, String), String> {
    let mut headers = vec![(
        "Authorization".to_string(),
        format!("Bearer {}", record.session_token),
    )];
    if body.is_some() {
        headers.push(("Content-Type".to_string(), "application/json".to_string()));
    }
    for &(name, value) in extra_headers {
        headers.push((name.to_string(), value.to_string()));
    }
    let header_refs: Vec<(&str, &str)> = headers
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    reach_text(
        &record.iroh_ticket,
        Some(record.daemon_url.as_str()),
        method,
        path,
        &header_refs,
        body,
    )
    .await
}

fn exchange_json(status: u16, body: &str) -> Result<String, String> {
    Ok(serde_json::json!({ "status": status, "body": body }).to_string())
}

/// Loopback invites talk to the daemon's own HTTP listener. A closed Iroh
/// relay then cannot leave the join parked on "Joining…". Remote invites
/// still dial the ticket, and that dial is bounded.
async fn reach_text(
    ticket: &str,
    http_base: Option<&str>,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Result<(u16, String), String> {
    if let Some(base) = http_base.and_then(crate::browser_portal_parse::loopback_http_origin) {
        match browser_http(&base, method, path, headers, body).await {
            Ok(response) => return Ok(response),
            Err(http_err) => {
                let iroh = iroh_text(ticket, method, path, headers, body).await;
                return match iroh {
                    Ok(response) => Ok(response),
                    Err(iroh_err) => Err(format!("{http_err} ({iroh_err})")),
                };
            }
        }
    }
    iroh_text(ticket, method, path, headers, body).await
}

async fn iroh_text(
    ticket: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Result<(u16, String), String> {
    let mut response = medousa_iroh_http::iroh_http_request(ticket, method, path, headers, body)
        .await
        .map_err(|err| format!("Cannot reach workshop over Iroh: {err}"))?;
    let text = read_body_text(&mut response.body).await?;
    Ok((response.status, text))
}

async fn browser_http(
    base: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> Result<(u16, String), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let url = format!("{}{path}", base.trim_end_matches('/'));
    let header_list = web_sys::Headers::new().map_err(|_| "failed to build request headers")?;
    for (name, value) in headers {
        header_list
            .set(name, value)
            .map_err(|_| format!("failed to set header {name}"))?;
    }
    let init = web_sys::RequestInit::new();
    init.set_method(method);
    init.set_mode(web_sys::RequestMode::Cors);
    init.set_headers(&header_list);
    if let Some(body) = body {
        let text = String::from_utf8_lossy(body);
        init.set_body(&JsValue::from_str(&text));
    }
    let request = web_sys::Request::new_with_str_and_init(&url, &init)
        .map_err(|_| format!("invalid workshop URL {url}"))?;
    let window = web_sys::window().ok_or_else(|| "browser window is unavailable".to_string())?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| format!("Cannot reach workshop at {url}"))?;
    let response: web_sys::Response = response
        .dyn_into()
        .map_err(|_| "workshop response was not a Response".to_string())?;
    let status = response.status();
    let text = JsFuture::from(
        response
            .text()
            .map_err(|_| "workshop body was unreadable".to_string())?,
    )
    .await
    .map_err(|_| "workshop body read failed".to_string())?;
    Ok((status, text.as_string().unwrap_or_default()))
}

async fn read_body_text(body: &mut medousa_iroh_http::IrohHttpBody) -> Result<String, String> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.read_chunk().await.map_err(|err| err.to_string())? {
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

async fn iroh_json<T: for<'de> Deserialize<'de>>(
    ticket: &str,
    http_base: Option<&str>,
    method: &str,
    path: &str,
    body: Option<&serde_json::Value>,
    headers: &[(&str, &str)],
) -> Result<T, String> {
    let encoded = body
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|err| err.to_string())?;
    let mut owned = Vec::new();
    if encoded.is_some() {
        owned.push(("Content-Type".to_string(), "application/json".to_string()));
    }
    for &(name, value) in headers {
        owned.push((name.to_string(), value.to_string()));
    }
    let refs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let (status, text) =
        reach_text(ticket, http_base, method, path, &refs, encoded.as_deref()).await?;
    if !(200..300).contains(&status) {
        return Err(format!(
            "Workshop returned HTTP {status} for {method} {path}: {text}"
        ));
    }
    serde_json::from_str(&text).map_err(|err| format!("Invalid workshop response over Iroh: {err}"))
}

fn active_record() -> Result<PortalRecord, String> {
    let store = read_store();
    let id = store.active_workshop_id.trim();
    if id.is_empty() || id == PERSONAL_WORKSHOP_ID {
        return Err("No paired workshop is selected.".to_string());
    }
    store
        .portals
        .into_iter()
        .find(|portal| portal.workshop_id == id)
        .ok_or_else(|| "The selected workshop is no longer paired in this browser.".to_string())
}

async fn ensure_fresh_session(record: PortalRecord) -> Result<PortalRecord, String> {
    if !session_refresh_due(record.session_expires_at.as_deref()) {
        return Ok(record);
    }
    refresh_session(record).await
}

async fn refresh_session(record: PortalRecord) -> Result<PortalRecord, String> {
    let _guard = REFRESH_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let stored = active_record().unwrap_or_else(|_| record.clone());
    if stored.workshop_id == record.workshop_id
        && stored.session_token != record.session_token
        && !session_refresh_due(stored.session_expires_at.as_deref())
    {
        return Ok(stored);
    }
    let identity = load_or_create_identity()?;
    if identity.phone_id != record.phone_id {
        return Err("The paired device identity changed; paste the invite again.".to_string());
    }
    let challenge_body = serde_json::json!({
        "pairingId": record.pairing_id,
        "phoneId": record.phone_id,
    });
    let challenge: PairSessionChallengePayload = iroh_json(
        &record.iroh_ticket,
        Some(record.daemon_url.as_str()),
        "POST",
        "/pair/session/challenge",
        Some(&challenge_body),
        &[],
    )
    .await?;
    if challenge.status != "challenge" {
        return Err(session_refresh_failure(challenge.reason.as_deref()));
    }
    let session_id = challenge
        .session_id
        .as_deref()
        .ok_or_else(|| "Workshop refresh challenge omitted its session id.".to_string())?;
    let server_nonce = challenge
        .server_nonce
        .as_deref()
        .ok_or_else(|| "Workshop refresh challenge omitted its nonce.".to_string())?;
    let mut phone_nonce = [0u8; 32];
    OsRng.fill_bytes(&mut phone_nonce);
    let phone_nonce_b64 = base64url_encode(&phone_nonce);
    let challenge_message = format!(
        "medousa-session-refresh-v1|{session_id}|{}|{}|{server_nonce}|{phone_nonce_b64}",
        record.pairing_id, record.phone_id
    );
    let signed_nonce = sign_message(&identity.signing_key, &challenge_message);
    let refresh_body = serde_json::json!({
        "sessionId": session_id,
        "pairingId": record.pairing_id,
        "phoneId": record.phone_id,
        "signedNonce": signed_nonce,
        "phoneNonce": phone_nonce_b64,
    });
    let refresh: PairSessionRefreshPayload = iroh_json(
        &record.iroh_ticket,
        Some(record.daemon_url.as_str()),
        "POST",
        "/pair/session/refresh",
        Some(&refresh_body),
        &[],
    )
    .await?;
    if refresh.status != "refreshed" {
        return Err(session_refresh_failure(refresh.reason.as_deref()));
    }
    let session_token = refresh
        .session_token
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Workshop refreshed the session without returning a token.".to_string())?;
    let session_expires_at = refresh
        .session_expires_at
        .ok_or_else(|| "Workshop refreshed the session without returning an expiry.".to_string())?;
    let expiry = chrono::DateTime::parse_from_rfc3339(&session_expires_at)
        .map_err(|_| "Workshop returned an invalid session expiry.".to_string())?;
    let issued_message = format!(
        "medousa-session-issued-v1|{session_id}|{}|{phone_nonce_b64}|{session_token}|{}",
        record.pairing_id,
        expiry.timestamp()
    );
    let server_signature = refresh
        .server_signed_nonce
        .as_deref()
        .ok_or_else(|| "Workshop refreshed the session without signing the result.".to_string())?;
    let daemon_key = parse_verifying_key(&record.daemon_public_key)
        .map_err(|err| format!("Workshop public key is invalid: {err}"))?;
    verify_message(&daemon_key, &issued_message, server_signature)
        .map_err(|err| format!("Workshop session signature check failed: {err}"))?;

    let mut updated = record;
    updated.session_token = session_token;
    updated.session_expires_at = Some(session_expires_at);
    let mut store = read_store();
    if let Some(existing) = store
        .portals
        .iter_mut()
        .find(|portal| portal.workshop_id == updated.workshop_id)
    {
        *existing = updated.clone();
    }
    write_store(&store)?;
    Ok(updated)
}

fn session_refresh_due(expires_at: Option<&str>) -> bool {
    let Some(expires_at) =
        expires_at.and_then(|value| chrono::DateTime::parse_from_rfc3339(value.trim()).ok())
    else {
        return false;
    };
    expires_at.with_timezone(&chrono::Utc)
        <= chrono::Utc::now() + chrono::Duration::seconds(SESSION_REFRESH_SKEW_SECONDS)
}

fn parse_invite(raw: &str) -> Result<ParsedInvite, String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("medousa://pair/") {
        return Err("Paste a Medousa pairing invite.".to_string());
    }
    let iroh_ticket = query_param(trimmed, "k").ok_or_else(|| {
        "This invite has no Iroh ticket. On the workshop, run `medousa pair qr --full` and paste that link."
            .to_string()
    })?;
    Ok(ParsedInvite {
        advertise_address: query_param(trimmed, "a")
            .ok_or_else(|| "Pairing link is missing an address.".to_string())?,
        device_id: query_param(trimmed, "d")
            .ok_or_else(|| "Pairing link is missing a device id.".to_string())?,
        qr_token: query_param(trimmed, "t")
            .ok_or_else(|| "Pairing link is missing a token.".to_string())?,
        signature: query_param(trimmed, "s")
            .ok_or_else(|| "Pairing link is missing a signature.".to_string())?,
        peer_name: query_param(trimmed, "n").unwrap_or_else(|| "Medousa".to_string()),
        daemon_public_key: query_param(trimmed, "u"),
        iroh_ticket,
        profile_id: query_param(trimmed, "p"),
    })
}

fn normalize_daemon_url(address: &str) -> String {
    let trimmed = address.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

fn load_or_create_identity() -> Result<DeviceIdentity, String> {
    let storage = local_storage()?;
    if let Some(existing) = storage
        .get_item(DEVICE_SECRET_KEY)
        .map_err(|_| "Could not read the browser device key.".to_string())?
        .filter(|value| !value.trim().is_empty())
    {
        return identity_from_hex(existing.trim());
    }
    let signing_key = SigningKey::generate(&mut OsRng);
    let encoded = encode_hex(signing_key.to_bytes().as_slice());
    storage
        .set_item(DEVICE_SECRET_KEY, &encoded)
        .map_err(|_| "Could not store the browser device key.".to_string())?;
    Ok(identity_from_key(signing_key))
}

fn identity_from_hex(raw: &str) -> Result<DeviceIdentity, String> {
    if raw.len() != 64 {
        return Err("Browser device key must be 32 bytes.".to_string());
    }
    let mut seed = [0u8; 32];
    for (index, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&raw[index * 2..index * 2 + 2], 16)
            .map_err(|_| "Browser device key is not hex.".to_string())?;
    }
    Ok(identity_from_key(SigningKey::from_bytes(&seed)))
}

fn identity_from_key(signing_key: SigningKey) -> DeviceIdentity {
    let verifying_key = signing_key.verifying_key();
    DeviceIdentity {
        phone_id: device_id_from_public_key(verifying_key.as_bytes()),
        signing_key,
        verifying_key,
    }
}

fn read_store() -> PortalStore {
    let Ok(storage) = local_storage() else {
        return PortalStore::default();
    };
    let Ok(Some(raw)) = storage.get_item(PORTAL_STORE_KEY) else {
        return PortalStore::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn write_store(store: &PortalStore) -> Result<(), String> {
    let storage = local_storage()?;
    let raw = serde_json::to_string(store).map_err(|err| err.to_string())?;
    storage
        .set_item(PORTAL_STORE_KEY, &raw)
        .map_err(|_| "Could not store the paired workshop.".to_string())
}

fn local_storage() -> Result<web_sys::Storage, String> {
    let window = web_sys::window().ok_or_else(|| "Browser window is unavailable.".to_string())?;
    window
        .local_storage()
        .map_err(|_| "Browser storage is unavailable.".to_string())?
        .ok_or_else(|| "Browser storage is unavailable.".to_string())
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn init_failure_message(reason: Option<&str>) -> String {
    match reason.unwrap_or("unknown") {
        "rate_limited" => "Too many pairing attempts — wait a minute and try again.".to_string(),
        "invalid_invite" | "token_expired" | "token_already_used" | "invalid_token" => {
            "This pairing invite is invalid, expired, or already used — paste a fresh one."
                .to_string()
        }
        "no_active_qr" => {
            "No active invite on the workshop — create a fresh one and paste it again.".to_string()
        }
        other => format!("Pairing was rejected ({other})"),
    }
}

fn verify_failure_message(reason: Option<&str>) -> String {
    match reason.unwrap_or("unknown") {
        "unknown_session" => {
            "Pairing session expired — paste a fresh invite and try again.".to_string()
        }
        other => format!("Pairing verify was rejected ({other})"),
    }
}

fn session_refresh_failure(reason: Option<&str>) -> String {
    match reason.unwrap_or("unknown") {
        "revoked" | "expired" => {
            "This browser is no longer trusted by the workshop — paste a fresh invite.".to_string()
        }
        other => format!("Could not refresh the workshop session ({other})"),
    }
}
