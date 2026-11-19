//! Trusted-peer conversations for desktop renderers. No discovery, pairing, or implicit sends.
//!
//! The local SDK retains the caller's selected loopback origin and credentials.
//! Outbound peers use the same registry, token store, signing identity and LAN/Iroh
//! transport as Tauri. Snapshot failures are per-source, never an empty-success fiction.
use crate::{mesh_envelope, pairing_client, workshop_registry, workshop_transport};
use medousa_sdk::MedousaClient;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedPeer {
    pub workshop_id: String,
    pub label: String,
    pub workshop_device_id: String,
    pub inbound: bool,
    pub has_session_token: bool,
    pub has_iroh_ticket: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerSnapshot {
    pub peers: Vec<TrustedPeer>,
    /// Existing peer message schema, with exact `sinkKind` and `workshopId` routing.
    pub messages: Vec<Value>,
    pub errors: Vec<String>,
}

pub struct PeerService {
    local: std::sync::Arc<MedousaClient>,
}
impl PeerService {
    pub fn new(local: impl Into<std::sync::Arc<MedousaClient>>) -> Self {
        Self {
            local: local.into(),
        }
    }

    fn check_local(&self) -> Result<(), String> {
        crate::connection::local_url(self.local.base_url()).map(|_| ())
    }

    async fn status(&self) -> Result<Value, String> {
        self.local
            .http()
            .get("/pair/status")
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn snapshot(&self) -> Result<PeerSnapshot, String> {
        self.snapshot_with_registry(read_registry()).await
    }

    async fn snapshot_with_registry(
        &self,
        registry: Result<workshop_registry::WorkshopRegistry, String>,
    ) -> Result<PeerSnapshot, String> {
        self.check_local()?;
        let mut out = PeerSnapshot::default();
        match self.status().await {
            Ok(status) => match inbound_peers(&status) {
                Ok(peers) => out.peers.extend(peers),
                Err(e) => out.errors.push(format!("Local peer trust: {e}")),
            },
            Err(e) => out.errors.push(format!("Local peer trust: {e}")),
        }
        match self.local.http().get::<Value>("/v1/peer/messages").await {
            Ok(value) => match message_array(&value) {
                Ok(messages) => out
                    .messages
                    .extend(messages.iter().cloned().map(|m| annotate(m, "host", ""))),
                Err(e) => out.errors.push(format!("Local messages: {e}")),
            },
            Err(e) => out.errors.push(format!("Local messages: {e}")),
        }
        let registry = match registry {
            Ok(registry) => registry,
            Err(e) => {
                out.errors.push(format!("Trusted peer registry: {e}"));
                return Ok(out);
            }
        };
        let mut mirrored_local = std::collections::HashSet::new();
        for entry in registry
            .workshops
            .iter()
            .filter(|e| workshop_registry::is_peer_kind(&e.kind))
        {
            let Some(pairing) = &entry.pairing else {
                out.errors
                    .push(format!("{}: missing pairing metadata", entry.label));
                continue;
            };
            let config =
                pairing_client::load_workshop_transport_config_for_id(&entry.id, &entry.url);
            out.peers.push(TrustedPeer {
                workshop_id: entry.id.clone(),
                label: entry.label.clone(),
                workshop_device_id: pairing.workshop_device_id.clone(),
                inbound: false,
                has_session_token: config.as_ref().is_some_and(|c| c.session_token.is_some()),
                has_iroh_ticket: config.as_ref().is_some_and(|c| c.iroh_ticket.is_some()),
            });
            let Some(config) = config else {
                out.errors
                    .push(format!("{}: credentials unavailable", entry.label));
                continue;
            };
            if config.session_token.is_none() {
                out.errors.push(format!(
                    "{}: session token unavailable; pair again",
                    entry.label
                ));
                continue;
            }
            match workshop_transport::workshop_get_json::<Value>(&config, "/v1/peer/messages").await
            {
                Ok(value) => match message_array(&value) {
                    Ok(messages) => {
                        for message in messages {
                            if let Some(mapped) = map_remote(
                                message,
                                &entry.id,
                                &entry.label,
                                &pairing.workshop_device_id,
                            ) {
                                // Local outbound records are authoritative. Remote inbox copies of
                                // our deliveries have different IDs, so ID-only dedup is insufficient.
                                if let Some(index) = out
                                    .messages
                                    .iter()
                                    .enumerate()
                                    .find(|(i, local)| {
                                        !mirrored_local.contains(i) && same_delivery(local, &mapped)
                                    })
                                    .map(|(i, _)| i)
                                {
                                    mirrored_local.insert(index);
                                } else {
                                    out.messages.push(mapped);
                                }
                            }
                        }
                    }
                    Err(e) => out.errors.push(format!("{} messages: {e}", entry.label)),
                },
                Err(e) => out.errors.push(format!("{} messages: {e}", entry.label)),
            }
        }
        out.messages
            .sort_by(|a, b| field(a, "sentAt").cmp(field(b, "sentAt")));
        Ok(out)
    }

    /// One explicit delivery only. A transport failure can mean delivery is unknown;
    /// callers must never automatically retry, including local-history failures.
    pub async fn send(&self, workshop_id: &str, body: &str) -> Result<Value, String> {
        self.check_local()?;
        if body.trim().is_empty() {
            return Err("Message is empty".into());
        }
        if let Some(device_id) = workshop_id.strip_prefix("inbound-") {
            let status = self.status().await?;
            let peer = inbound_peers(&status)?
                .into_iter()
                .find(|p| p.workshop_device_id == device_id)
                .ok_or("Inbound peer is no longer trusted")?;
            return self.local.http().post("/v1/peer/messages", &json!({
                "body":body,"toDeviceId":peer.workshop_device_id,"toName":peer.label,"direction":"out"
            })).await.map_err(|e| format!("Delivery outcome unknown; do not automatically retry: {e}"));
        }
        let registry = read_registry()?;
        let entry = registry
            .workshops
            .iter()
            .find(|e| e.id == workshop_id && workshop_registry::is_peer_kind(&e.kind))
            .ok_or("Trusted peer not found")?;
        let pairing = entry
            .pairing
            .as_ref()
            .ok_or("Peer pairing metadata unavailable")?;
        let config = pairing_client::load_workshop_transport_config_for_id(&entry.id, &entry.url)
            .ok_or("Peer credentials unavailable")?;
        if config.session_token.is_none()
            || config
                .daemon_public_key
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
        {
            return Err("Peer session or pinned identity unavailable; pair again".into());
        }
        // Fail before delivery if local identity is unavailable; do not forge a sender.
        let status = self.status().await?;
        let from_id = field(&status, "deviceId");
        if from_id.is_empty() {
            return Err("Local workshop identity unavailable".into());
        }
        let from_name = field(&status, "peerName");
        let signed = mesh_envelope::wrap_json_for_workshop(
            &config,
            mesh_envelope::CAP_MESH_MESSAGE,
            json!({"body":body,"fromDeviceId":from_id,"fromName":from_name}),
        )?;
        workshop_transport::workshop_post_json_once::<Value, _>(
            &config,
            "/v1/peer/messages",
            &signed,
        )
        .await
        .map_err(|e| {
            format!("Peer delivery failed or outcome unknown; do not automatically retry: {e}")
        })?;
        self.local
            .http()
            .post(
                "/v1/peer/messages",
                &json!({
                    "body":body,"fromDeviceId":from_id,"fromName":from_name,
                    "toDeviceId":pairing.workshop_device_id,"toName":entry.label,"direction":"out"
                }),
            )
            .await
            .map_err(|e| {
                format!(
                    "Delivered to peer, but local history could not be saved. Do not resend: {e}"
                )
            })
    }

    /// Route only to the exact source supplied by snapshot, never search other peers.
    pub async fn mark_read(&self, message: &Value) -> Result<Value, String> {
        self.check_local()?;
        let id = field(message, "id");
        if id.is_empty() {
            return Err("Message id missing".into());
        }
        let path = format!("/v1/peer/messages/{}/read", urlencoding::encode(id));
        match field(message, "sinkKind") {
            "host" => self
                .local
                .http()
                .post(&path, &json!({}))
                .await
                .map_err(|e| e.to_string()),
            "peer" => {
                let registry = read_registry()?;
                let entry = registry
                    .workshops
                    .iter()
                    .find(|e| {
                        e.id == field(message, "workshopId")
                            && workshop_registry::is_peer_kind(&e.kind)
                    })
                    .ok_or("Message peer is no longer trusted")?;
                let config =
                    pairing_client::load_workshop_transport_config_for_id(&entry.id, &entry.url)
                        .ok_or("Peer credentials unavailable")?;
                workshop_transport::workshop_post_json_once(&config, &path, &json!({})).await
            }
            _ => Err("Unknown message source".into()),
        }
    }
}

fn read_registry() -> Result<workshop_registry::WorkshopRegistry, String> {
    let path = workshop_registry::workshops_registry_path();
    let registry = match std::fs::read(path) {
        Ok(raw) => serde_json::from_slice(&raw).map_err(|e| format!("Invalid registry: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => workshop_registry::default_registry(),
        Err(e) => return Err(e.to_string()),
    };
    workshop_registry::validate_registry(&registry)?;
    Ok(registry)
}
fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
fn message_array(value: &Value) -> Result<&Vec<Value>, String> {
    value
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "Malformed messages response".into())
}
fn inbound_peers(status: &Value) -> Result<Vec<TrustedPeer>, String> {
    let devices = status
        .get("pairedDevices")
        .and_then(Value::as_array)
        .ok_or("Malformed peer trust response")?;
    Ok(devices
        .iter()
        .filter(|d| field(d, "role") == "peer" && !field(d, "phoneId").is_empty())
        .map(|d| TrustedPeer {
            workshop_id: format!("inbound-{}", field(d, "phoneId")),
            label: field(d, "phoneName").to_string(),
            workshop_device_id: field(d, "phoneId").to_string(),
            inbound: true,
            has_session_token: true,
            has_iroh_ticket: false,
        })
        .collect())
}
fn annotate(mut message: Value, sink: &str, workshop: &str) -> Value {
    if let Some(m) = message.as_object_mut() {
        m.insert("sinkKind".into(), json!(sink));
        m.insert("workshopId".into(), json!(workshop));
    }
    message
}
fn map_remote(message: &Value, workshop: &str, label: &str, device: &str) -> Option<Value> {
    let mut mapped = annotate(message.clone(), "peer", workshop);
    let m = mapped.as_object_mut()?;
    match field(message, "direction") {
        "out" => {
            m.insert("direction".into(), json!("in"));
            m.insert("fromDeviceId".into(), json!(device));
            m.insert("fromName".into(), json!(label));
        }
        "in" => {
            m.insert("direction".into(), json!("out"));
            m.insert("toDeviceId".into(), json!(device));
            m.insert("toName".into(), json!(label));
        }
        _ => return None,
    }
    Some(mapped)
}
fn same_delivery(a: &Value, b: &Value) -> bool {
    if field(a, "sinkKind") != "host"
        || field(a, "direction") != "out"
        || field(b, "direction") != "out"
    {
        return false;
    }
    if ["toDeviceId", "body"]
        .iter()
        .any(|k| field(a, k).is_empty() || field(a, k) != field(b, k))
    {
        return false;
    }
    // Each server assigns its own ID/time. Match mirrors one-to-one within a
    // bounded clock/dispatch window; repeated identical sends stay separate.
    match (
        chrono::DateTime::parse_from_rfc3339(field(a, "sentAt")),
        chrono::DateTime::parse_from_rfc3339(field(b, "sentAt")),
    ) {
        (Ok(a), Ok(b)) => (a - b).num_milliseconds().abs() <= 5000,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
    };

    // Only ephemeral mock sockets; no daemon, credentials, trust, or real peers.
    fn mock(
        replies: Vec<(&'static str, Value)>,
    ) -> (PeerService, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            replies.into_iter().map(|(path,body)| {
                let (mut stream,_)=listener.accept().unwrap();
                stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                let mut raw=Vec::new();
                loop { let mut bytes=[0;4096];let n=stream.read(&mut bytes).unwrap();raw.extend_from_slice(&bytes[..n]);
                    if n==0 || raw.windows(4).any(|w| w==b"\r\n\r\n") {break;}
                }
                let request=String::from_utf8_lossy(&raw).to_string();
                assert!(request.lines().next().unwrap().contains(path),"{request}");
                assert!(request.to_ascii_lowercase().contains("authorization: bearer mock-secret"));
                let body=body.to_string();
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
                request
            }).collect()
        });
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            "Bearer mock-secret".parse().unwrap(),
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();
        let sdk = MedousaClient::with_transport(
            Arc::new(medousa_sdk::HttpTransport::with_client(http)),
            base,
        );
        (PeerService::new(sdk), server)
    }
    #[test]
    fn remote_threads_reverse_directions_and_keep_exact_source() {
        let m = map_remote(
            &json!({"id":"x","direction":"out","body":"hi"}),
            "peer-a",
            "A",
            "device-a",
        )
        .unwrap();
        assert_eq!(m["direction"], "in");
        assert_eq!(m["fromDeviceId"], "device-a");
        assert_eq!(m["workshopId"], "peer-a");
        let sent = map_remote(&json!({"direction":"in"}), "peer-a", "A", "device-a").unwrap();
        assert_eq!(sent["direction"], "out");
        assert_eq!(sent["toDeviceId"], "device-a");
        assert!(map_remote(&json!({"direction":"unknown"}), "a", "A", "d").is_none());
    }
    #[test]
    fn mirror_matching_is_bounded_to_same_recipient_body_and_time() {
        let local = json!({"sinkKind":"host","direction":"out","toDeviceId":"p","body":"same","sentAt":"2026-11-19T12:00:00Z"});
        let mut remote = json!({"sinkKind":"peer","direction":"out","toDeviceId":"p","body":"same","sentAt":"2026-11-19T12:00:01Z"});
        assert!(same_delivery(&local, &remote));
        remote["sentAt"] = json!("2026-11-19T12:01:00Z");
        assert!(!same_delivery(&local, &remote));
        remote["sentAt"] = json!("2026-11-19T12:00:01Z");
        remote["toDeviceId"] = json!("other");
        assert!(!same_delivery(&local, &remote));
    }

    #[test]
    fn only_exact_trusted_inbound_peers_are_send_targets() {
        let peers=inbound_peers(&json!({"pairedDevices":[{"role":"portal","phoneId":"a"},{"role":"peer","phoneId":"ab","phoneName":"AB"},{"role":"peer"}]})).unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].workshop_id, "inbound-ab");
        assert!(inbound_peers(&json!({})).is_err());
        assert!(message_array(&json!({})).is_err());
    }
    #[tokio::test]
    async fn snapshot_retains_messages_with_partial_trust_and_registry_errors() {
        let (service, server) = mock(vec![
            ("/pair/status", json!({})),
            (
                "/v1/peer/messages",
                json!({"messages":[{"id":"m","direction":"in"}]}),
            ),
        ]);
        let out = service
            .snapshot_with_registry(Err("fixture registry failure".into()))
            .await
            .unwrap();
        assert_eq!(out.messages.len(), 1);
        assert_eq!(out.messages[0]["sinkKind"], "host");
        assert_eq!(out.errors.len(), 2);
        assert_eq!(server.join().unwrap().len(), 2);
    }
    #[tokio::test]
    async fn inbound_send_validates_trust_and_preserves_authenticated_sdk() {
        let (service, server) = mock(vec![
            (
                "/pair/status",
                json!({"pairedDevices":[{"phoneId":"peer1","phoneName":"One","role":"peer"}]}),
            ),
            ("/v1/peer/messages", json!({"id":"sent"})),
        ]);
        assert_eq!(
            service.send("inbound-peer1", "hello").await.unwrap()["id"],
            "sent"
        );
        let requests = server.join().unwrap();
        assert!(requests[1].starts_with("POST "));
    }
    #[tokio::test]
    async fn stale_inbound_trust_never_posts() {
        let (service, server) = mock(vec![("/pair/status", json!({"pairedDevices":[]}))]);
        assert!(
            service
                .send("inbound-missing", "hello")
                .await
                .unwrap_err()
                .contains("no longer trusted")
        );
        assert_eq!(server.join().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn mark_read_uses_encoded_id_and_exact_source() {
        let (service, server) = mock(vec![(
            "/v1/peer/messages/a%2Fb/read",
            json!({"id":"a/b","readAt":"now"}),
        )]);
        service
            .mark_read(&json!({"id":"a/b","sinkKind":"host"}))
            .await
            .unwrap();
        assert_eq!(server.join().unwrap().len(), 1);
        assert!(
            service
                .mark_read(&json!({"id":"m","sinkKind":"unknown"}))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn remote_local_client_and_empty_body_fail_before_effects() {
        let service = PeerService::new(MedousaClient::new("https://remote.invalid"));
        assert!(
            service
                .snapshot_with_registry(Ok(workshop_registry::default_registry()))
                .await
                .is_err()
        );
        assert!(service.send("inbound-x", "hi").await.is_err());
        let service = PeerService::new(MedousaClient::new("http://127.0.0.1:1"));
        assert!(
            service
                .send("inbound-x", "  ")
                .await
                .unwrap_err()
                .contains("empty")
        );
    }
}
