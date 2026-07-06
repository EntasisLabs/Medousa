//! Aggregates peer conversations from host local, portal remote, and outbound peer sinks.

use std::collections::HashSet;

use crate::daemon::DaemonState;
use crate::pairing_client::WorkshopTransportConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedWorkshopSummary {
    pub workshop_id: String,
    pub label: String,
    pub daemon_url: String,
    pub workshop_device_id: String,
    pub paired_at: String,
    pub has_session_token: bool,
    pub has_iroh_ticket: bool,
    #[serde(default)]
    pub inbound: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerSendMessageRequest {
    pub workshop_id: String,
    pub body: String,
    #[serde(default)]
    pub attachment: Option<serde_json::Value>,
}

const SINK_HOST: &str = "host";
const SINK_PORTAL: &str = "portal";
const SINK_PEER: &str = "peer";

pub fn is_host_engine(state: &State<'_, DaemonState>) -> bool {
    let Ok(base) = daemon_base(state) else {
        return false;
    };
    let host = if let Ok(url) = url::Url::parse(&base) {
        url.host_str().unwrap_or("").to_string()
    } else {
        base.trim_start_matches("http://")
            .trim_start_matches("https://")
            .split(['/', ':'])
            .next()
            .unwrap_or("")
            .to_string()
    };
    matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

pub fn daemon_base(state: &State<'_, DaemonState>) -> Result<String, String> {
    Ok(state
        .daemon_url
        .lock()
        .expect("daemon url lock")
        .clone()
        .trim_end_matches('/')
        .to_string())
}

pub fn daemon_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())
}

struct ActivePortalSink {
    workshop_id: String,
    label: String,
    config: WorkshopTransportConfig,
}

struct PeerInboxContext {
    is_host: bool,
    host_base: Option<String>,
    host_workshop_id: String,
    portal: Option<ActivePortalSink>,
}

impl PeerInboxContext {
    fn build(state: &State<'_, DaemonState>) -> Result<Self, String> {
        let is_host = is_host_engine(state);
        let host_base = if is_host {
            Some(daemon_base(state)?)
        } else {
            None
        };
        let registry = crate::workshop_registry::load_registry()?;
        let host_workshop_id = registry.active_workshop_id.clone();
        let portal = if is_host {
            None
        } else {
            active_portal_sink(&registry)?
        };
        Ok(Self {
            is_host,
            host_base,
            host_workshop_id,
            portal,
        })
    }
}

fn active_portal_sink(
    registry: &crate::workshop_registry::WorkshopRegistry,
) -> Result<Option<ActivePortalSink>, String> {
    let active = registry
        .workshops
        .iter()
        .find(|entry| entry.id == registry.active_workshop_id);
    let Some(workshop) = active else {
        return Ok(None);
    };
    if !crate::workshop_registry::is_portal_kind(&workshop.kind) {
        return Ok(None);
    }
    let Some(config) = crate::pairing_client::load_workshop_transport_config_for_id(
        &workshop.id,
        &workshop.url,
    ) else {
        return Ok(None);
    };
    if config.session_token.is_none() {
        return Ok(None);
    }
    Ok(Some(ActivePortalSink {
        workshop_id: workshop.id.clone(),
        label: workshop.label.clone(),
        config,
    }))
}

fn annotate_message(
    mut message: serde_json::Value,
    sink_kind: &str,
    workshop_id: &str,
) -> serde_json::Value {
    if let Some(object) = message.as_object_mut() {
        object.insert("sinkKind".into(), serde_json::json!(sink_kind));
        object.insert("workshopId".into(), serde_json::json!(workshop_id));
    }
    message
}

fn device_ids_match(left: &str, right: &str) -> bool {
    if left.is_empty() || right.is_empty() {
        return left == right;
    }
    left == right
        || left.starts_with(&right[..right.len().min(8)])
        || right.starts_with(&left[..left.len().min(8)])
}

fn message_involves_device(message: &serde_json::Value, device_id: &str) -> bool {
    let from = message
        .get("fromDeviceId")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let to = message
        .get("toDeviceId")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    device_ids_match(from, device_id) || device_ids_match(to, device_id)
}

fn is_inbound_unread(message: &serde_json::Value) -> bool {
    message.get("direction").and_then(|v| v.as_str()) != Some("out")
        && message.get("readAt").and_then(|v| v.as_str()).is_none()
}

pub async fn list_messages(
    state: &State<'_, DaemonState>,
    unread_only: bool,
) -> Result<Vec<serde_json::Value>, String> {
    let ctx = PeerInboxContext::build(state)?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    if ctx.is_host {
        if let Some(base) = &ctx.host_base {
            let mut url = format!("{base}/v1/peer/messages");
            if unread_only {
                url.push_str("?unreadOnly=true");
            }
            let client = daemon_http_client()?;
            if let Ok(response) = client.get(&url).send().await {
                if response.status().is_success() {
                    if let Ok(payload) = response.json::<serde_json::Value>().await {
                        if let Some(messages) = payload.get("messages").and_then(|v| v.as_array()) {
                            for message in messages {
                                let id = message
                                    .get("id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                if !id.is_empty() {
                                    seen.insert(id);
                                }
                                out.push(annotate_message(
                                    message.clone(),
                                    SINK_HOST,
                                    &ctx.host_workshop_id,
                                ));
                            }
                        }
                    }
                }
            }
        }
        let remote = fetch_outbound_peer_conversations(unread_only, false).await;
        for message in remote {
            merge_message(&mut out, &mut seen, message);
        }
    } else {
        if let Some(portal) = &ctx.portal {
            let path = if unread_only {
                "/v1/peer/messages?unreadOnly=true"
            } else {
                "/v1/peer/messages"
            };
            if let Ok(payload) =
                crate::workshop_transport::workshop_get_json::<serde_json::Value>(
                    &portal.config,
                    path,
                )
                .await
            {
                if let Some(messages) = payload.get("messages").and_then(|v| v.as_array()) {
                    for message in messages {
                        let id = message
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !id.is_empty() {
                            seen.insert(id);
                        }
                        out.push(annotate_message(
                            message.clone(),
                            SINK_PORTAL,
                            &portal.workshop_id,
                        ));
                    }
                }
            }
        }
        let remote = fetch_outbound_peer_conversations(unread_only, true).await;
        for message in remote {
            merge_message(&mut out, &mut seen, message);
        }
    }

    Ok(out)
}

fn merge_message(
    out: &mut Vec<serde_json::Value>,
    seen: &mut HashSet<String>,
    message: serde_json::Value,
) {
    let id = message
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if id.is_empty() || seen.insert(id) {
        out.push(message);
    }
}

async fn fetch_outbound_peer_conversations(
    unread_only: bool,
    full_thread: bool,
) -> Vec<serde_json::Value> {
    let Ok(registry) = crate::workshop_registry::load_registry() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for workshop in registry.workshops {
        if !crate::workshop_registry::is_peer_kind(&workshop.kind) {
            continue;
        }
        let Some(pairing) = workshop.pairing.as_ref() else {
            continue;
        };
        let Some(config) = crate::pairing_client::load_workshop_transport_config_for_id(
            &workshop.id,
            &workshop.url,
        ) else {
            continue;
        };
        let path = if unread_only {
            "/v1/peer/messages?unreadOnly=true"
        } else {
            "/v1/peer/messages"
        };
        let Ok(payload) =
            crate::workshop_transport::workshop_get_json::<serde_json::Value>(&config, path).await
        else {
            continue;
        };
        let Some(messages) = payload.get("messages").and_then(|value| value.as_array()) else {
            continue;
        };
        for message in messages {
            let direction = message
                .get("direction")
                .and_then(|value| value.as_str())
                .unwrap_or("in");
            if direction == "out" {
                if unread_only && message.get("readAt").and_then(|value| value.as_str()).is_some()
                {
                    continue;
                }
                let mut mapped = message.clone();
                if let Some(object) = mapped.as_object_mut() {
                    object.insert("direction".into(), serde_json::json!("in"));
                    object.insert(
                        "fromDeviceId".into(),
                        serde_json::json!(pairing.workshop_device_id),
                    );
                    object.insert("fromName".into(), serde_json::json!(workshop.label));
                    object.insert("sinkKind".into(), serde_json::json!(SINK_PEER));
                    object.insert("workshopId".into(), serde_json::json!(workshop.id));
                }
                out.push(mapped);
            } else if full_thread && direction == "in" {
                if unread_only {
                    continue;
                }
                let mut mapped = message.clone();
                if let Some(object) = mapped.as_object_mut() {
                    object.insert("direction".into(), serde_json::json!("out"));
                    object.insert(
                        "toDeviceId".into(),
                        serde_json::json!(pairing.workshop_device_id),
                    );
                    object.insert("toName".into(), serde_json::json!(workshop.label));
                    object.insert("sinkKind".into(), serde_json::json!(SINK_PEER));
                    object.insert("workshopId".into(), serde_json::json!(workshop.id));
                }
                out.push(mapped);
            }
        }
    }
    out
}

pub async fn unread_count(state: &State<'_, DaemonState>) -> Result<usize, String> {
    let messages = list_messages(state, true).await?;
    Ok(messages.len())
}

pub async fn mark_read(
    state: &State<'_, DaemonState>,
    message_id: String,
    sink_kind: Option<String>,
    workshop_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let ctx = PeerInboxContext::build(state)?;
    let sink = sink_kind.as_deref().unwrap_or("");
    let workshop = workshop_id.as_deref().unwrap_or("");

    if sink == SINK_HOST || (sink.is_empty() && ctx.is_host) {
        if let Some(base) = &ctx.host_base {
            let client = daemon_http_client()?;
            if let Ok(response) = client
                .post(format!("{base}/v1/peer/messages/{message_id}/read"))
                .send()
                .await
            {
                if response.status().is_success() {
                    return response.json().await.map_err(|err| err.to_string());
                }
            }
        }
    }

    if sink == SINK_PORTAL || (sink.is_empty() && ctx.portal.is_some()) {
        if let Some(portal) = &ctx.portal {
            if workshop.is_empty() || workshop == portal.workshop_id {
                if let Ok(value) = crate::workshop_transport::workshop_post_json::<
                    serde_json::Value,
                    _,
                >(
                    &portal.config,
                    &format!("/v1/peer/messages/{message_id}/read"),
                    &serde_json::json!({}),
                )
                .await
                {
                    return Ok(value);
                }
            }
        }
    }

    if sink == SINK_PEER || sink.is_empty() {
        let Ok(registry) = crate::workshop_registry::load_registry() else {
            return Err("message not found".to_string());
        };
        for entry in registry.workshops {
            if !crate::workshop_registry::is_peer_kind(&entry.kind) {
                continue;
            }
            if !workshop.is_empty() && entry.id != workshop {
                continue;
            }
            let Some(config) = crate::pairing_client::load_workshop_transport_config_for_id(
                &entry.id,
                &entry.url,
            ) else {
                continue;
            };
            if let Ok(value) = crate::workshop_transport::workshop_post_json::<
                serde_json::Value,
                _,
            >(
                &config,
                &format!("/v1/peer/messages/{message_id}/read"),
                &serde_json::json!({}),
            )
            .await
            {
                return Ok(value);
            }
        }
    }

    Err("message not found".to_string())
}

pub async fn mark_thread_read(
    state: &State<'_, DaemonState>,
    peer_device_id: String,
) -> Result<usize, String> {
    let messages = list_messages(state, false).await?;
    let mut marked = 0usize;
    for message in messages {
        if !message_involves_device(&message, &peer_device_id) || !is_inbound_unread(&message) {
            continue;
        }
        let id = message
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        let sink_kind = message
            .get("sinkKind")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let workshop_id = message
            .get("workshopId")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if mark_read(state, id, sink_kind, workshop_id).await.is_ok() {
            marked += 1;
        }
    }
    Ok(marked)
}

async fn fetch_pair_status_local(state: &State<'_, DaemonState>) -> Result<serde_json::Value, String> {
    let base = daemon_base(state)?;
    let client = daemon_http_client()?;
    let response = client
        .get(format!("{base}/pair/status"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        return Err(format!("pair status HTTP {}", response.status()));
    }
    response.json().await.map_err(|err| err.to_string())
}

async fn fetch_pair_status_for_context(
    state: &State<'_, DaemonState>,
    ctx: &PeerInboxContext,
) -> Result<serde_json::Value, String> {
    if ctx.is_host {
        return fetch_pair_status_local(state).await;
    }
    if let Some(portal) = &ctx.portal {
        return crate::workshop_transport::workshop_get_json::<serde_json::Value>(
            &portal.config,
            "/pair/status",
        )
        .await;
    }
    Err("No workshop inbox available".to_string())
}

pub async fn append_inbound_peers(
    state: &State<'_, DaemonState>,
    out: &mut Vec<TrustedWorkshopSummary>,
    known_device_ids: &mut Vec<String>,
) -> Result<(), String> {
    let ctx = PeerInboxContext::build(state)?;
    if !ctx.is_host && ctx.portal.is_none() {
        return Ok(());
    }
    let status = fetch_pair_status_for_context(state, &ctx).await?;
    if let Some(devices) = status.get("pairedDevices").and_then(|v| v.as_array()) {
        for device in devices {
            let role = device
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("portal");
            if role != "peer" {
                continue;
            }
            let phone_id = device
                .get("phoneId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if phone_id.is_empty() {
                continue;
            }
            if known_device_ids.iter().any(|id| device_ids_match(id, &phone_id)) {
                continue;
            }
            let label = device
                .get("phoneName")
                .and_then(|v| v.as_str())
                .filter(|value| !value.is_empty())
                .unwrap_or("Peer")
                .to_string();
            let paired_at = device
                .get("pairedAt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            out.push(TrustedWorkshopSummary {
                workshop_id: format!("inbound-{phone_id}"),
                label,
                daemon_url: String::new(),
                workshop_device_id: phone_id,
                paired_at,
                has_session_token: true,
                has_iroh_ticket: false,
                inbound: true,
            });
        }
    }
    Ok(())
}

pub async fn send_message(
    state: &State<'_, DaemonState>,
    request: PeerSendMessageRequest,
) -> Result<serde_json::Value, String> {
    let ctx = PeerInboxContext::build(state)?;
    let (from_device_id, from_name) = local_identity_with_ctx(state, &ctx).await;
    let (to_device_id, to_name, remote_config, inbound) =
        resolve_send_target(state, &ctx, &request.workshop_id).await?;

    if inbound {
        let body = serde_json::json!({
            "body": request.body,
            "toDeviceId": to_device_id,
            "toName": to_name,
            "direction": "out",
            "attachment": request.attachment,
        });
        if ctx.is_host {
            if let Some(base) = &ctx.host_base {
                let client = daemon_http_client()?;
                let response = client
                    .post(format!("{base}/v1/peer/messages"))
                    .json(&body)
                    .send()
                    .await
                    .map_err(|err| format!("failed to record sent message: {err}"))?;
                if !response.status().is_success() {
                    let status = response.status();
                    let text = response.text().await.unwrap_or_default();
                    return Err(format!("failed to record sent message HTTP {status}: {text}"));
                }
                return response.json().await.map_err(|err| err.to_string());
            }
        } else if let Some(portal) = &ctx.portal {
            return crate::workshop_transport::workshop_post_json::<serde_json::Value, _>(
                &portal.config,
                "/v1/peer/messages",
                &body,
            )
            .await;
        }
        return Err("Messaging requires a workshop inbox connection".to_string());
    }

    let deliver_body = serde_json::json!({
        "body": request.body,
        "fromDeviceId": from_device_id,
        "fromName": from_name,
        "attachment": request.attachment,
    });

    if let Some(config) = remote_config {
        crate::workshop_transport::workshop_post_json::<serde_json::Value, _>(
            &config,
            "/v1/peer/messages",
            &deliver_body,
        )
        .await?;
    } else if !ctx.is_host && ctx.portal.is_none() {
        return Err("Messaging requires a peer connection from this device".to_string());
    }

    if ctx.is_host {
        let local_body = serde_json::json!({
            "body": request.body,
            "fromDeviceId": from_device_id,
            "fromName": from_name,
            "toDeviceId": to_device_id,
            "toName": to_name,
            "direction": "out",
            "attachment": request.attachment,
        });
        if let Some(base) = &ctx.host_base {
            let client = daemon_http_client()?;
            let response = client
                .post(format!("{base}/v1/peer/messages"))
                .json(&local_body)
                .send()
                .await
                .map_err(|err| format!("failed to record sent message: {err}"))?;
            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                return Err(format!("failed to record sent message HTTP {status}: {text}"));
            }
            return response.json().await.map_err(|err| err.to_string());
        }
    }

    Ok(serde_json::json!({
        "id": format!("out_{}", uuid::Uuid::new_v4()),
        "body": request.body,
        "fromDeviceId": from_device_id,
        "fromName": from_name,
        "toDeviceId": to_device_id,
        "toName": to_name,
        "direction": "out",
        "sentAt": chrono::Utc::now().to_rfc3339(),
        "readAt": null,
        "attachment": request.attachment,
        "sinkKind": SINK_PEER,
        "workshopId": request.workshop_id,
    }))
}

async fn resolve_send_target(
    state: &State<'_, DaemonState>,
    ctx: &PeerInboxContext,
    workshop_id: &str,
) -> Result<
    (
        String,
        String,
        Option<WorkshopTransportConfig>,
        bool,
    ),
    String,
> {
    if let Some(phone_id) = workshop_id.strip_prefix("inbound-") {
        let status = fetch_pair_status_for_context(state, ctx).await?;
        let device = status
            .get("pairedDevices")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .find(|entry| {
                entry.get("phoneId").and_then(|v| v.as_str()) == Some(phone_id)
                    && entry.get("role").and_then(|v| v.as_str()).unwrap_or("portal") == "peer"
            })
            .ok_or_else(|| format!("Inbound peer '{phone_id}' not found"))?;
        let label = device
            .get("phoneName")
            .and_then(|v| v.as_str())
            .unwrap_or("Peer")
            .to_string();
        return Ok((phone_id.to_string(), label, None, true));
    }

    let registry = crate::workshop_registry::load_registry()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|entry| entry.id == workshop_id)
        .ok_or_else(|| format!("Unknown workshop '{workshop_id}'"))?;
    if !crate::workshop_registry::is_peer_kind(&workshop.kind) {
        return Err("Messaging requires a peer connection".to_string());
    }
    let pairing = workshop
        .pairing
        .as_ref()
        .ok_or_else(|| "Peer credentials are missing".to_string())?;
    let config = crate::pairing_client::load_workshop_transport_config_for_id(
        workshop_id,
        &workshop.url,
    )
    .ok_or_else(|| "Trusted workshop credentials are missing or expired".to_string())?;
    Ok((
        pairing.workshop_device_id.clone(),
        workshop.label.clone(),
        Some(config),
        false,
    ))
}

pub async fn local_identity(state: &State<'_, DaemonState>) -> (String, String) {
    let ctx = PeerInboxContext::build(state).unwrap_or(PeerInboxContext {
        is_host: false,
        host_base: None,
        host_workshop_id: crate::workshop_registry::PERSONAL_WORKSHOP_ID.to_string(),
        portal: None,
    });
    local_identity_with_ctx(state, &ctx).await
}

async fn local_identity_with_ctx(
    state: &State<'_, DaemonState>,
    ctx: &PeerInboxContext,
) -> (String, String) {
    if !ctx.is_host && ctx.portal.is_none() {
        if let Ok(identity) = crate::pairing_client::client_surface_identity() {
            return identity;
        }
        return ("client".to_string(), "Medousa".to_string());
    }

    if let Ok(status) = fetch_pair_status_for_context(state, ctx).await {
        let device_id = status
            .get("deviceId")
            .and_then(|v| v.as_str())
            .unwrap_or("local")
            .to_string();
        let peer_name = status
            .get("peerName")
            .and_then(|v| v.as_str())
            .unwrap_or("Medousa")
            .to_string();
        return (device_id, peer_name);
    }

    ("local".to_string(), "Medousa".to_string())
}

pub async fn workshop_display_name(state: &State<'_, DaemonState>) -> String {
    let (_, name) = local_identity(state).await;
    name
}

pub async fn client_surface_name(state: &State<'_, DaemonState>) -> String {
    let ctx = PeerInboxContext::build(state).ok();
    if let Some(ctx) = ctx {
        if !ctx.is_host {
            if let Ok((_, name)) = crate::pairing_client::client_surface_identity() {
                return name;
            }
        }
    }
    "Medousa".to_string()
}
