//! Daemon-owned outbound peer authority. Desktop renderers never receive credentials.
use crate::pairing::{PairingService, crypto::*};
use anyhow::{Context, Result, bail};
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};
use chrono::{DateTime, Utc};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

const MAX_BYTES: usize = 4 * 1024 * 1024;
static STORE_LOCK: Mutex<()> = Mutex::new(());
static CEREMONY_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection {
    device_id: String,
    label: String,
    pairing_id: String,
    public_key: String,
    lan_base: String,
    iroh_ticket: Option<String>,
    session_token: String,
    session_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSummary {
    pub workshop_id: String,
    pub label: String,
    pub workshop_device_id: String,
    pub pairing_id: String,
    pub inbound: bool,
    pub has_session_token: bool,
    pub has_iroh_ticket: bool,
}
impl Connection {
    fn summary(&self) -> ConnectionSummary {
        ConnectionSummary {
            workshop_id: self.device_id.clone(),
            label: self.label.clone(),
            workshop_device_id: self.device_id.clone(),
            pairing_id: self.pairing_id.clone(),
            inbound: false,
            has_session_token: !self.session_token.is_empty(),
            has_iroh_ticket: self.iroh_ticket.is_some(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairRequest {
    pub qr_url: Option<String>,
    pub daemon_url: String,
    pub short_code: Option<String>,
    /// Required for short-code pairing to prevent silently trusting an arbitrary status response.
    pub expected_device_id: Option<String>,
}

pub async fn store_io<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::StoreIo,
            medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
            move || Ok(operation()),
        )
        .await?
}
fn key(service: &PairingService) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"medousa/daemon-outbound-peers/v1\0");
    digest.update(service.identity().signing_key().to_bytes());
    digest.finalize().into()
}
fn encrypt(key: &[u8; 32], rows: &[Connection]) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec(rows)?;
    if bytes.len() > MAX_BYTES {
        bail!("outbound peer storage capacity exceeded");
    }
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = XChaCha20Poly1305::new(key.into())
        .encrypt(XNonce::from_slice(&nonce), bytes.as_slice())
        .map_err(|_| anyhow::anyhow!("encrypt peer credentials"))?;
    Ok([nonce.to_vec(), encrypted].concat())
}
fn decrypt(key: &[u8; 32], bytes: &[u8]) -> Result<Vec<Connection>> {
    if bytes.len() < 40 || bytes.len() > MAX_BYTES + 40 {
        bail!("invalid outbound credential store");
    }
    let plain = XChaCha20Poly1305::new(key.into())
        .decrypt(XNonce::from_slice(&bytes[..24]), &bytes[24..])
        .map_err(|_| {
            anyhow::anyhow!("outbound credentials cannot be decrypted with daemon identity")
        })?;
    serde_json::from_slice(&plain).context("decode outbound peer credentials")
}
fn store_root() -> Result<crate::store_root::StoreRoot> {
    Ok(crate::store_root::StoreRoot::open_or_create_nofollow(
        &crate::pairing::paths::pairings_dir(),
    )?)
}
fn store_path() -> crate::store_root::StorePath {
    crate::store_root::StorePath::parse("outbound.enc").expect("static outbound credential path")
}
fn read(service: &PairingService) -> Result<Vec<Connection>> {
    let root = store_root()?;
    let path = store_path();
    if !root.is_file(&path)? {
        return Ok(vec![]);
    }
    decrypt(
        &key(service),
        &root.read_limited(&path, (MAX_BYTES + 40) as u64)?,
    )
}
fn mutate<T>(
    service: &PairingService,
    f: impl FnOnce(&mut Vec<Connection>) -> Result<T>,
) -> Result<T> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("peer storage lock poisoned"))?;
    let mut rows = read(service)?;
    let value = f(&mut rows)?;
    store_root()?.atomic_write(&store_path(), &encrypt(&key(service), &rows)?)?;
    Ok(value)
}
pub async fn list(service: Arc<PairingService>) -> Result<Vec<ConnectionSummary>> {
    store_io(move || {
        let _guard = STORE_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("peer storage lock poisoned"))?;
        Ok(read(&service)?.iter().map(Connection::summary).collect())
    })
    .await
}
async fn load(service: Arc<PairingService>, device_id: String) -> Result<Connection> {
    store_io(move || {
        let _guard = STORE_LOCK
            .lock()
            .map_err(|_| anyhow::anyhow!("peer storage lock poisoned"))?;
        read(&service)?
            .into_iter()
            .find(|r| r.device_id == device_id)
            .context("outbound peer not found; pair this daemon first")
    })
    .await
}
pub async fn forget(
    service: Arc<PairingService>,
    device_id: String,
    pairing_id: String,
) -> Result<()> {
    let _guard = CEREMONY_LOCK.lock().await;
    store_io(move || {
        mutate(&service, |rows| {
            let index = rows
                .iter()
                .position(|r| r.device_id == device_id && r.pairing_id == pairing_id)
                .context("peer identity changed; refresh before forgetting")?;
            rows.remove(index);
            Ok(())
        })
    })
    .await
}
fn normalize_url(raw: &str) -> Result<String> {
    let normalized = if raw.contains("://") {
        raw.trim().to_string()
    } else {
        format!("http://{}", raw.trim())
    };
    let url = reqwest::Url::parse(&normalized)?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        bail!("peer address must be an HTTP(S) origin without credentials or path");
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .with_context(|| format!("peer response omitted {name}"))
}
async fn wire(
    base: &str,
    ticket: Option<&str>,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<&Value>,
) -> Result<Value> {
    let mut headers = vec![("content-type", "application/json".to_string())];
    if let Some(token) = token {
        headers.push(("authorization", format!("Bearer {token}")));
    }
    let bytes = body.map(serde_json::to_vec).transpose()?;
    // Select a route before issuing a mutation. Never retry a failed POST on a second transport.
    #[cfg(feature = "iroh-transport")]
    if let Some(ticket) = ticket {
        let refs: Vec<_> = headers.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let mut response =
            medousa_iroh_http::iroh_http_request(ticket, method, path, &refs, bytes.as_deref())
                .await?;
        if !(200..300).contains(&response.status) {
            bail!(
                "peer HTTP {} (delivery may be unknown; not retried)",
                response.status
            );
        }
        let mut out = Vec::new();
        while let Some(chunk) = response.body.read_chunk().await? {
            if out.len() + chunk.len() > MAX_BYTES {
                bail!("peer response too large");
            }
            out.extend(chunk);
        }
        return serde_json::from_slice(&out).context("decode peer response");
    }
    #[cfg(not(feature = "iroh-transport"))]
    let _ = ticket;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(30))
        .build()?;
    let mut request = client.request(method.parse()?, format!("{base}{path}"));
    for (name, value) in headers {
        request = request.header(name, value);
    }
    if let Some(bytes) = bytes {
        request = request.body(bytes);
    }
    let mut response = request
        .send()
        .await
        .context("peer request failed; delivery may be unknown and was not retried")?;
    if !response.status().is_success() {
        bail!("peer HTTP {} (not retried)", response.status().as_u16());
    }
    let mut out = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if out.len() + chunk.len() > MAX_BYTES {
            bail!("peer response too large");
        }
        out.extend(chunk);
    }
    serde_json::from_slice(&out).context("decode peer response")
}

pub async fn pair(service: Arc<PairingService>, request: PairRequest) -> Result<ConnectionSummary> {
    let _guard = CEREMONY_LOCK.lock().await;
    let base = normalize_url(&request.daemon_url)?;
    let qr = request
        .qr_url
        .as_ref()
        .map(|q| reqwest::Url::parse(q))
        .transpose()?;
    let query = |name: &str| {
        qr.as_ref().and_then(|q| {
            q.query_pairs()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.into_owned())
        })
    };
    if qr.as_ref().is_some_and(|q| {
        q.scheme() != "medousa"
            || q.host_str() != Some("pair")
            || !matches!(q.path(), "/1.0" | "/2.0")
    }) {
        bail!("invalid Medousa pairing link");
    }
    let ticket = query("k");
    let status = match (query("u"), query("d")) {
        (Some(public), Some(device)) => {
            json!({"daemonPublicKey":public,"deviceId":device,"peerName":query("n").unwrap_or_else(||"Medousa".into())})
        }
        _ => wire(&base, ticket.as_deref(), "GET", "/pair/status", None, None).await?,
    };
    let public = field(&status, "daemonPublicKey")?;
    let device = field(&status, "deviceId")?;
    let verifying = parse_verifying_key(public)?;
    if device_id_from_public_key(verifying.as_bytes()) != device || device == service.device_id() {
        bail!("peer identity mismatch or self pairing");
    }
    if let Some(qr) = &qr {
        let address = query("a").context("pairing link missing address")?;
        let qr_device = query("d").context("pairing link missing device id")?;
        let token = query("t").context("pairing link missing token")?;
        let signature = query("s").context("pairing link missing signature")?;
        if qr_device != device {
            bail!("pairing link identity mismatch");
        }
        let profile = query("p");
        if qr.path() == "/2.0" {
            verify_qr_url_signature_v2_with_profile(
                &verifying,
                &address,
                device,
                &token,
                ticket
                    .as_deref()
                    .context("v2 pairing link missing ticket")?,
                profile.as_deref(),
                &signature,
            )?;
        } else {
            verify_qr_url_signature_with_profile(
                &verifying,
                &address,
                device,
                &token,
                profile.as_deref(),
                &signature,
            )?;
        }
    } else if request.expected_device_id.as_deref() != Some(device)
        || request
            .short_code
            .as_deref()
            .is_none_or(|v| v.trim().is_empty())
    {
        bail!("short-code pairing requires expected device identity and code");
    }
    let init = wire(&base,ticket.as_deref(),"POST","/pair/init",None,Some(&json!({
        "qrToken":query("t"),"shortCode":request.short_code,"phoneId":service.device_id(),"phoneName":service.peer_name(),"publicKey":base64url_encode(service.identity().verifying_key().as_bytes()),"role":"peer"
    }))).await?;
    if field(&init, "status")? != "challenge" && init.get("serverNonce").is_none() {
        bail!("peer declined pairing ceremony");
    }
    let session = field(&init, "sessionId")?;
    let mut random = [0u8; 32];
    OsRng.fill_bytes(&mut random);
    let nonce = base64url_encode(&random);
    let proof = wire(&base,ticket.as_deref(),"POST","/pair/verify",None,Some(&json!({"sessionId":session,"signedNonce":sign_message(service.identity().signing_key(),field(&init,"serverNonce")?),"phoneNonce":nonce}))).await?;
    if field(&proof, "status")? != "paired" {
        bail!("peer declined pairing proof");
    }
    verify_message(&verifying, &nonce, field(&proof, "serverSignedNonce")?)?;
    let mut row = Connection {
        device_id: device.into(),
        label: status
            .get("peerName")
            .and_then(Value::as_str)
            .unwrap_or("Medousa")
            .into(),
        pairing_id: field(&proof, "pairingId")?.into(),
        public_key: public.into(),
        lan_base: base,
        iroh_ticket: ticket,
        session_token: field(&proof, "sessionToken")?.into(),
        session_expires_at: DateTime::parse_from_rfc3339(field(&proof, "sessionExpiresAt")?)?
            .with_timezone(&Utc),
    };
    if row.iroh_ticket.is_none() {
        if let Ok(info) = wire(
            &row.lan_base,
            None,
            "GET",
            "/pair/iroh-ticket",
            Some(&row.session_token),
            None,
        )
        .await
        {
            row.iroh_ticket = info
                .get("ticket")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }
    let summary = row.summary();
    store_io(move || {
        mutate(&service, |rows| {
            if let Some(old) = rows.iter().find(|old| old.device_id == row.device_id) {
                if old.public_key != row.public_key {
                    bail!("peer key changed; forget old trust explicitly first");
                }
            }
            crate::mesh::registry::register_outbound_identity(
                &row.device_id,
                &row.label,
                &row.public_key,
                &row.pairing_id,
                crate::mesh::MeshPeerEndpoints {
                    lan_base_url: Some(row.lan_base.clone()),
                    iroh_ticket: row.iroh_ticket.clone(),
                    iroh_endpoint_id: None,
                },
            )?;
            rows.retain(|old| old.device_id != row.device_id);
            rows.push(row);
            Ok(())
        })
    })
    .await?;
    Ok(summary)
}

async fn refresh(service: Arc<PairingService>, device_id: String) -> Result<Connection> {
    let _guard = CEREMONY_LOCK.lock().await;
    let mut row = load(service.clone(), device_id).await?;
    if row.session_expires_at > Utc::now() + chrono::Duration::seconds(60) {
        return Ok(row);
    }
    let challenge = wire(
        &row.lan_base,
        row.iroh_ticket.as_deref(),
        "POST",
        "/pair/session/challenge",
        None,
        Some(&json!({"pairingId":row.pairing_id,"phoneId":service.device_id()})),
    )
    .await?;
    let session = field(&challenge, "sessionId")?;
    let server_nonce = field(&challenge, "serverNonce")?;
    let mut random = [0u8; 32];
    OsRng.fill_bytes(&mut random);
    let nonce = base64url_encode(&random);
    let message = format!(
        "medousa-session-refresh-v1|{session}|{}|{}|{server_nonce}|{nonce}",
        row.pairing_id,
        service.device_id()
    );
    let proof = wire(&row.lan_base,row.iroh_ticket.as_deref(),"POST","/pair/session/refresh",None,Some(&json!({"sessionId":session,"pairingId":row.pairing_id,"phoneId":service.device_id(),"signedNonce":sign_message(service.identity().signing_key(),&message),"phoneNonce":nonce}))).await?;
    if field(&proof, "status")? != "refreshed" {
        bail!("peer session refresh declined");
    }
    let token = field(&proof, "sessionToken")?;
    let expiry =
        DateTime::parse_from_rfc3339(field(&proof, "sessionExpiresAt")?)?.with_timezone(&Utc);
    let issued = format!(
        "medousa-session-issued-v1|{session}|{}|{nonce}|{token}|{}",
        row.pairing_id,
        expiry.timestamp()
    );
    verify_message(
        &parse_verifying_key(&row.public_key)?,
        &issued,
        field(&proof, "serverSignedNonce")?,
    )?;
    row.session_token = token.into();
    row.session_expires_at = expiry;
    let updated = row.clone();
    store_io(move || {
        mutate(&service, |rows| {
            let stored = rows
                .iter_mut()
                .find(|r| r.device_id == updated.device_id && r.pairing_id == updated.pairing_id)
                .context("peer removed during session refresh")?;
            stored.session_token = updated.session_token;
            stored.session_expires_at = updated.session_expires_at;
            Ok(())
        })
    })
    .await?;
    Ok(row)
}

pub async fn get(service: Arc<PairingService>, device: &str, path: &str) -> Result<Value> {
    let row = refresh(service, device.into()).await?;
    wire(
        &row.lan_base,
        row.iroh_ticket.as_deref(),
        "GET",
        path,
        Some(&row.session_token),
        None,
    )
    .await
}
pub async fn message(service: Arc<PairingService>, device: &str, payload: Value) -> Result<Value> {
    let row = refresh(service.clone(), device.into()).await?;
    let device = device.to_string();
    let seq = store_io(move || crate::mesh::registry::allocate_outbound_seq(&device)).await?;
    let envelope = crate::mesh::sign_envelope(
        service.identity().signing_key(),
        service.device_id(),
        &row.device_id,
        seq,
        crate::mesh::MeshCapability::Message,
        &crate::mesh::payload_hash_hex(&payload)?,
        chrono::Duration::seconds(crate::mesh::DEFAULT_ENVELOPE_TTL_SECS),
    );
    wire(
        &row.lan_base,
        row.iroh_ticket.as_deref(),
        "POST",
        "/v1/peer/messages",
        Some(&row.session_token),
        Some(&json!({"envelope":envelope,"payload":payload})),
    )
    .await
}

#[cfg(test)]
#[path = "peer_connections_protocol_tests.rs"]
mod protocol_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_store_authenticates_identity_and_detects_corruption() {
        let row = Connection {
            device_id: "peer".into(),
            label: "Peer".into(),
            pairing_id: "pair".into(),
            public_key: "key".into(),
            lan_base: "http://peer:7419".into(),
            iroh_ticket: None,
            session_token: "secret-must-not-be-public".into(),
            session_expires_at: Utc::now(),
        };
        let encoded = encrypt(&[7; 32], &[row]).unwrap();
        assert!(!encoded.windows(9).any(|w| w == b"secret-mu"));
        assert_eq!(decrypt(&[7; 32], &encoded).unwrap()[0].device_id, "peer");
        assert!(decrypt(&[8; 32], &encoded).is_err());
        let mut corrupt = encoded;
        corrupt[25] ^= 1;
        assert!(decrypt(&[7; 32], &corrupt).is_err());
        assert!(decrypt(&[7; 32], &[]).is_err());
    }
    #[test]
    fn outbound_origin_rejects_credential_and_path_injection() {
        for bad in [
            "file:///etc/passwd",
            "http://user:pass@peer",
            "http://peer/path",
            "http://peer?token=x",
            "http://peer#x",
        ] {
            assert!(normalize_url(bad).is_err(), "{bad}");
        }
        assert_eq!(normalize_url("peer:7419").unwrap(), "http://peer:7419");
    }
}
