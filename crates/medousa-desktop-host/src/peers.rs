//! Thin peer UI adapter. The selected daemon owns identities, credentials,
//! connections, routing, message aggregation and attachment authorization.
//! Never read the legacy client registry or contact a peer from this module.
use medousa_sdk::MedousaClient;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

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
    pub messages: Vec<Value>,
    pub errors: Vec<String>,
}

pub struct PeerService {
    local: Arc<MedousaClient>,
}
impl PeerService {
    pub fn new(local: impl Into<Arc<MedousaClient>>) -> Self {
        Self {
            local: local.into(),
        }
    }

    pub async fn snapshot(&self) -> Result<PeerSnapshot, String> {
        self.local
            .http()
            .get("/v1/peers/snapshot")
            .await
            .map_err(|e| e.to_string())
    }

    /// No transport fallback or implicit retries: a failed response can mean
    /// delivery succeeded but the acknowledgement was lost.
    pub async fn send(&self, workshop_id: &str, body: &str) -> Result<Value, String> {
        if workshop_id.is_empty() || body.trim().is_empty() {
            return Err("Choose a peer and enter a message".into());
        }
        self.post("send", &json!({"workshopId":workshop_id,"body":body}))
            .await
            .map_err(|e| {
                format!("Delivery outcome may be unknown; do not automatically retry: {e}")
            })
    }

    pub async fn share(
        &self,
        workshop_id: &str,
        item: &str,
        artifact: bool,
        mode: &str,
    ) -> Result<Value, String> {
        if workshop_id.is_empty()
            || item.trim().is_empty()
            || !matches!(mode, "share" | "ask" | "bring")
        {
            return Err("Choose one peer, item and valid share mode".into());
        }
        self.post(
            "share",
            &json!({"id":workshop_id,"item":item,"artifact":artifact,"mode":mode}),
        )
        .await
    }

    pub async fn open_attachment(
        &self,
        message: &Value,
        item: &str,
        artifact: bool,
    ) -> Result<Value, String> {
        let mut request = message_source(message)?;
        request["item"] = json!(item);
        request["artifact"] = json!(artifact);
        self.post("attachment", &request).await
    }

    pub async fn mark_read(&self, message: &Value) -> Result<Value, String> {
        self.post("read", &message_source(message)?).await
    }

    async fn post(&self, operation: &str, payload: &Value) -> Result<Value, String> {
        self.local
            .http()
            .post(&format!("/v1/peers/{operation}"), payload)
            .await
            .map_err(|e| e.to_string())
    }
}

// Send only the source selector. The daemon must re-read the stored message
// and validate the imported recipient locator, not trust UI attachment data.
fn message_source(message: &Value) -> Result<Value, String> {
    let id = message["id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("Message id missing")?;
    let sink = message["sinkKind"]
        .as_str()
        .ok_or("Unknown message source")?;
    let workshop = message["workshopId"].as_str().unwrap_or("");
    if !matches!(sink, "host" | "peer") || (sink == "peer" && workshop.is_empty()) {
        return Err("Unknown message source".into());
    }
    Ok(json!({"messageId":id,"sinkKind":sink,"workshopId":workshop}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_selector_never_forwards_remote_attachment_claims() {
        let source = message_source(&json!({"id":"a/b","sinkKind":"peer","workshopId":"peer-1","attachmentResult":{"imported":true,"vaultPaths":["forged"]}})).unwrap();
        assert_eq!(
            source,
            json!({"messageId":"a/b","sinkKind":"peer","workshopId":"peer-1"})
        );
        for message in [
            json!({"id":"x"}),
            json!({"id":"x","sinkKind":"peer"}),
            json!({"id":"","sinkKind":"host"}),
        ] {
            assert!(message_source(&message).is_err());
        }
    }
}
