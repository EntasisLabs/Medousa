//! Explicit peer lifecycle operations. Discovery is not trust; pairing is peer-only.
use crate::{pairing_client, workshop_registry as registry, workshop_transport};
use medousa_sdk::MedousaClient;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
pub enum Request {
    Status,
    Discover,
    Invite,
    Pair {
        invite: String,
        url: String,
        name: String,
    },
    Forget {
        id: String,
        device: String,
        pairing: String,
    },
    Policy {
        device: String,
        pairing: String,
        expires: Option<String>,
        idle: Option<u64>,
    },
}
impl Request {
    pub fn mutation(&self) -> bool {
        matches!(
            self,
            Self::Pair { .. } | Self::Forget { .. } | Self::Policy { .. }
        )
    }
}
pub struct PeerManagementService {
    local: Arc<MedousaClient>,
}
impl PeerManagementService {
    pub fn new(local: Arc<MedousaClient>) -> Self {
        Self { local }
    }
    async fn status(&self) -> Result<Value, String> {
        let status: Value = self
            .local
            .http()
            .get("/pair/status")
            .await
            .map_err(|e| e.to_string())?;
        if !status["pairedDevices"].is_array() {
            return Err("Malformed pairing status".into());
        }
        Ok(status)
    }
    pub async fn execute(&self, request: &Request) -> Result<Value, String> {
        crate::connection::local_url(self.local.base_url())?;
        match request {
            Request::Status => {
                let status = self.status().await?;
                let mut peers = Vec::new();
                for p in status["pairedDevices"].as_array().unwrap() {
                    if p["role"].as_str() != Some("peer") {
                        continue;
                    }
                    let device = required(p, "phoneId")?;
                    peers.push(json!({"id":format!("inbound-{device}"), "device":device,
                        "name":p["phoneName"], "pairing":required(p,"pairingId")?, "inbound":true,
                        "active":p["trustActive"], "expires":p["trustExpiresAt"], "idle":p["idleTimeoutSeconds"]}));
                }
                let r = registry::load_registry()?;
                registry::validate_registry(&r)?;
                for p in r
                    .workshops
                    .iter()
                    .filter(|p| registry::is_peer_kind(&p.kind))
                {
                    let Some(pair) = &p.pairing else {
                        continue;
                    };
                    peers.push(
                        json!({"id":p.id,"device":pair.workshop_device_id,"name":p.label,
                        "pairing":pair.pairing_id,"inbound":false,"url":p.url}),
                    );
                }
                Ok(
                    json!({"peers":peers,"deviceId":status["deviceId"],"peerName":status["peerName"],"qrActive":status["qrActive"]}),
                )
            }
            Request::Discover => {
                let v: Value = self
                    .local
                    .http()
                    .get("/v1/lan/workshops")
                    .await
                    .map_err(|e| e.to_string())?;
                if !v["workshops"].is_array() {
                    return Err("Malformed discovery response".into());
                }
                Ok(v)
            }
            Request::Invite => {
                let v: Value = self
                    .local
                    .http()
                    .get("/qr?full=true")
                    .await
                    .map_err(|e| e.to_string())?;
                required(&v, "url")?;
                required(&v, "expiresAt")?;
                required(&v, "shortCode")?;
                Ok(v)
            }
            Request::Pair { invite, url, name } => {
                if invite.trim().is_empty() || url.trim().is_empty() {
                    return Err("An invite and workshop URL are required".into());
                }
                let paired = pairing_client::pair_complete_from_qr(
                    pairing_client::PairCompleteFromQrRequest {
                        qr_url: invite.clone(),
                        daemon_url: url.clone(),
                        phone_name: Some(name.clone()),
                        role: Some("peer".into()),
                    },
                )
                .await?;
                serde_json::to_value(paired).map_err(|e| e.to_string())
            }
            Request::Forget {
                id,
                device,
                pairing,
            } => {
                if id.starts_with("inbound-") {
                    let status = self.status().await?;
                    check_inbound(&status, device, pairing)?;
                    if id != &format!("inbound-{device}") {
                        return Err("Peer identity changed; refresh".into());
                    }
                    self.local
                        .http()
                        .delete::<Value>(&format!("/pair/{}", urlencoding::encode(pairing)))
                        .await
                        .map_err(|e| e.to_string())?;
                } else {
                    let mut r = registry::load_registry()?;
                    registry::validate_registry(&r)?;
                    check_outbound(&r, id, device, pairing)?;
                    pairing_client::remove_workshop_credentials(id, Some(device))?;
                    r.workshops.retain(|p| &p.id != id);
                    if r.active_workshop_id == *id {
                        r.active_workshop_id = registry::PERSONAL_WORKSHOP_ID.into();
                    }
                    registry::save_registry(&r)?;
                    workshop_transport::invalidate_all_route_caches();
                }
                Ok(json!({"removed":id}))
            }
            Request::Policy {
                device,
                pairing,
                expires,
                idle,
            } => {
                validate_policy(expires.as_deref(), *idle)?;
                let status = self.status().await?;
                check_inbound(&status, device, pairing)?;
                self.local
                    .http()
                    .put(
                        &format!("/pair/{}/policy", urlencoding::encode(pairing)),
                        &json!({"trustExpiresAt":expires,"idleTimeoutSeconds":idle}),
                    )
                    .await
                    .map_err(|e| e.to_string())
            }
        }
    }
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing peer {key}"))
}
fn check_inbound(status: &Value, device: &str, pairing: &str) -> Result<(), String> {
    if device.is_empty() || pairing.is_empty() {
        return Err("Missing peer identity".into());
    }
    status["pairedDevices"]
        .as_array()
        .ok_or("Malformed pairing status")?
        .iter()
        .find(|p| {
            p["phoneId"].as_str() == Some(device)
                && p["pairingId"].as_str() == Some(pairing)
                && p["role"].as_str() == Some("peer")
        })
        .map(|_| ())
        .ok_or_else(|| "Peer trust changed; refresh before trying again".into())
}
fn check_outbound(
    r: &registry::WorkshopRegistry,
    id: &str,
    device: &str,
    pairing: &str,
) -> Result<(), String> {
    r.workshops
        .iter()
        .find(|p| {
            p.id == id
                && registry::is_peer_kind(&p.kind)
                && p.pairing
                    .as_ref()
                    .is_some_and(|v| v.workshop_device_id == device && v.pairing_id == pairing)
        })
        .map(|_| ())
        .ok_or_else(|| "Peer trust changed; refresh before trying again".into())
}
fn validate_policy(expires: Option<&str>, idle: Option<u64>) -> Result<(), String> {
    if let Some(v) = expires {
        let date = chrono::DateTime::parse_from_rfc3339(v)
            .map_err(|_| "Use an RFC3339 expiry timestamp")?;
        if date <= chrono::Utc::now() {
            return Err("Trust expiration must be in the future".into());
        }
    }
    if idle.is_some_and(|n| !(86_400..=10 * 365 * 86_400).contains(&n)) {
        return Err("Inactivity timeout must be between 1 and 3650 days, or unset".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn mock(
        replies: Vec<(&'static str, Value)>,
    ) -> (PeerManagementService, std::thread::JoinHandle<Vec<String>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            replies.into_iter().map(|(path,body)| {
            let (mut stream,_)=listener.accept().unwrap();stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let mut raw=Vec::new();
            loop {let mut b=[0;4096];let n=stream.read(&mut b).unwrap();raw.extend_from_slice(&b[..n]);
                if let Some(end)=raw.windows(4).position(|w|w==b"\r\n\r\n") {
                    let header=String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();let len=header.lines().find_map(|l|l.strip_prefix("content-length:")).and_then(|s|s.trim().parse::<usize>().ok()).unwrap_or(0);
                    if raw.len()>=end+4+len {break;}
                }if n==0{break;}
            }
            let request=String::from_utf8_lossy(&raw).to_string();assert!(request.lines().next().unwrap().contains(path),"{request}");assert!(request.to_ascii_lowercase().contains("authorization: bearer fixture"));
            let body=body.to_string();write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();request
        }).collect()
        });
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            "Bearer fixture".parse().unwrap(),
        );
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();
        (
            PeerManagementService::new(Arc::new(MedousaClient::with_transport(
                Arc::new(medousa_sdk::HttpTransport::with_client(http)),
                base,
            ))),
            server,
        )
    }
    #[tokio::test]
    async fn incoming_removal_revalidates_identity_and_encodes_pairing() {
        let (s, server) = mock(vec![
            (
                "/pair/status",
                json!({"pairedDevices":[{"phoneId":"d","pairingId":"a/b","role":"peer"}]}),
            ),
            ("/pair/a%2Fb", json!({"status":"ok"})),
        ]);
        s.execute(&Request::Forget {
            id: "inbound-d".into(),
            device: "d".into(),
            pairing: "a/b".into(),
        })
        .await
        .unwrap();
        let requests = server.join().unwrap();
        assert!(requests[1].starts_with("DELETE "));
    }
    #[tokio::test]
    async fn stale_or_portal_identity_never_mutates() {
        for role in ["portal", "peer"] {
            let (s, server) = mock(vec![(
                "/pair/status",
                json!({"pairedDevices":[{"phoneId":"d","pairingId":"new","role":role}]}),
            )]);
            assert!(
                s.execute(&Request::Forget {
                    id: "inbound-d".into(),
                    device: "d".into(),
                    pairing: "old".into()
                })
                .await
                .is_err()
            );
            assert_eq!(server.join().unwrap().len(), 1);
        }
    }
    #[tokio::test]
    async fn policy_uses_authenticated_put_with_explicit_null_clears() {
        let (s, server) = mock(vec![
            (
                "/pair/status",
                json!({"pairedDevices":[{"phoneId":"d","pairingId":"p","role":"peer"}]}),
            ),
            ("/pair/p/policy", json!({"pairingId":"p"})),
        ]);
        s.execute(&Request::Policy {
            device: "d".into(),
            pairing: "p".into(),
            expires: None,
            idle: None,
        })
        .await
        .unwrap();
        let requests = server.join().unwrap();
        assert!(requests[1].starts_with("PUT "));
        assert!(requests[1].contains("\"trustExpiresAt\":null"));
        assert!(requests[1].contains("\"idleTimeoutSeconds\":null"));
    }
    #[tokio::test]
    async fn discovery_and_invites_only_fetch_and_validate() {
        let (s, server) = mock(vec![
            ("/v1/lan/workshops", json!({"workshops":[]})),
            (
                "/qr?full=true",
                json!({"url":"medousa://fixture","expiresAt":"soon","shortCode":"123456"}),
            ),
        ]);
        s.execute(&Request::Discover).await.unwrap();
        s.execute(&Request::Invite).await.unwrap();
        assert!(server.join().unwrap().iter().all(|r| r.starts_with("GET ")));
        let (s, server) = mock(vec![("/qr?full=true", json!({}))]);
        assert!(s.execute(&Request::Invite).await.is_err());
        server.join().unwrap();
        let s = PeerManagementService::new(Arc::new(MedousaClient::new("https://remote.invalid")));
        assert!(s.execute(&Request::Discover).await.is_err());
    }
    #[test]
    fn exact_peer_pairing_and_role_required() {
        let s = json!({"pairedDevices":[{"phoneId":"abc","pairingId":"pair1","role":"peer"},{"phoneId":"portal","pairingId":"pair2","role":"portal"}]});
        assert!(check_inbound(&s, "abc", "pair1").is_ok());
        for (d, p) in [
            ("ab", "pair1"),
            ("abc", "old"),
            ("portal", "pair2"),
            ("", "pair1"),
        ] {
            assert!(check_inbound(&s, d, p).is_err());
        }
    }
    #[test]
    fn policies_validate_before_writes() {
        assert!(validate_policy(None, None).is_ok());
        assert!(validate_policy(Some("2099-01-01T00:00:00Z"), Some(604800)).is_ok());
        assert!(validate_policy(Some("2020-01-01T00:00:00Z"), None).is_err());
        assert!(validate_policy(Some("tomorrow"), None).is_err());
        assert!(validate_policy(None, Some(0)).is_err());
    }
    #[test]
    fn outbound_cannot_remove_portal_or_repaired_identity() {
        let mut r = registry::default_registry();
        assert!(check_outbound(&r, "personal", "device", "pair").is_err());
        let mut p = registry::default_personal_workshop();
        p.id = "peer-1".into();
        p.kind = "peer".into();
        p.pairing = Some(registry::WorkshopPairingRef {
            pairing_id: "pair".into(),
            phone_id: "me".into(),
            workshop_device_id: "device".into(),
            paired_at: "now".into(),
            credentials_rel_path: None,
            has_iroh_ticket: Some(false),
            workshop_peer_name: None,
        });
        r.workshops.push(p);
        assert!(check_outbound(&r, "peer-1", "device", "pair").is_ok());
        assert!(check_outbound(&r, "peer-1", "device", "old").is_err());
    }
}
