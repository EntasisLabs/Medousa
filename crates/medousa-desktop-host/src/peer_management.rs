//! Peer lifecycle UI adapter. Network pairing and trust belong to the daemon.
//! The sole host-specific operation is lifecycle control of an exactly owned
//! local engine (LAN restart); it does not own a peer identity or connection.
use medousa_sdk::MedousaClient;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
pub enum Request {
    Status,
    Discover,
    Invite,
    QrImage,
    LanStatus,
    LanEnabled {
        enabled: bool,
    },
    Share {
        id: String,
        device: String,
        pairing: String,
        item: String,
        artifact: bool,
        mode: String,
    },
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
            Self::Pair { .. }
                | Self::Forget { .. }
                | Self::Policy { .. }
                | Self::Share { .. }
                | Self::LanEnabled { .. }
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

    pub async fn execute(&self, request: &Request) -> Result<Value, String> {
        match request {
            Request::Status => self.get("/v1/peers/connections").await,
            Request::Discover => self.get("/v1/lan/workshops").await,
            Request::Invite => self.get("/qr?full=true").await,
            Request::QrImage => self.get("/qr/image?full=true").await,
            Request::LanStatus => crate::peer_lan::status(self.local.base_url()).await,
            Request::LanEnabled { enabled } => crate::peer_lan::set_enabled(self.local.base_url(), *enabled).await,
            Request::Pair { invite, url, name } => {
                if invite.trim().is_empty() || url.trim().is_empty() {
                    return Err("An invite and workshop URL are required".into());
                }
                self.post("pair", &json!({"invite":invite,"url":url,"name":name})).await
            }
            Request::Forget { id, device, pairing } => self.post("forget", &json!({"id":id,"device":device,"pairing":pairing})).await,
            Request::Policy { device, pairing, expires, idle } => self.post("policy", &json!({"device":device,"pairing":pairing,"expires":expires,"idle":idle})).await,
            Request::Share { id, device, pairing, item, artifact, mode } => self.post("share", &json!({"id":id,"device":device,"pairing":pairing,"item":item,"artifact":artifact,"mode":mode})).await,
        }
    }
    async fn get(&self, path: &str) -> Result<Value, String> {
        self.local.http().get(path).await.map_err(|e| e.to_string())
    }
    async fn post(&self, operation: &str, payload: &Value) -> Result<Value, String> {
        self.local
            .http()
            .post(&format!("/v1/peers/{operation}"), payload)
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
#[path = "peer_api_tests.rs"]
mod tests;
