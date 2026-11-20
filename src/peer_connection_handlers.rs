//! Owner-facing peer operations. Only the selected daemon talks to other peers.
use crate::daemon::route_policy::{
    BrowserPolicy, DeclaredRouter, RateLimitClass, RouteGroup, RoutePolicy,
};
use crate::request_principal::{Capability, RequestPrincipal};
use crate::{pairing::PairingService, peer_connections as connections};
use anyhow::{Context, Result, bail};
use axum::{
    Json,
    extract::{Extension, State},
    http::{Method, StatusCode},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
pub struct PeerConnectionApiState {
    pub pairing: Option<Arc<PairingService>>,
}
type ApiResult = Result<Json<Value>, (StatusCode, String)>;
fn error(e: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, e.to_string())
}
fn owner(principal: &RequestPrincipal) -> Result<()> {
    if !principal.capabilities().contains(Capability::AdminIdentity) {
        bail!("peer connections require workshop owner/admin authority");
    }
    Ok(())
}
fn service(
    state: &PeerConnectionApiState,
    principal: &RequestPrincipal,
) -> Result<Arc<PairingService>> {
    owner(principal)?;
    state
        .pairing
        .clone()
        .context("daemon pairing service unavailable")
}
fn policy(method: Method, path: &'static str) -> RoutePolicy {
    RoutePolicy {
        method,
        path,
        group: RouteGroup::Administration,
        required_capability: Some(Capability::AdminIdentity),
        bootstrap_public: false,
        browser_policy: BrowserPolicy::NativeOnly,
        body_limit: 2 * 1024 * 1024,
        rate_limit_class: RateLimitClass::Administration,
    }
}
pub fn surface() -> DeclaredRouter<PeerConnectionApiState> {
    DeclaredRouter::default()
        .route(policy(Method::GET, "/pair/discover"), get(discover))
        .route(policy(Method::GET, "/v1/peers/connections"), get(status))
        .route(policy(Method::GET, "/v1/peers/snapshot"), get(snapshot))
        .route(policy(Method::POST, "/v1/peers/pair"), post(pair))
        .route(policy(Method::POST, "/v1/peers/send"), post(send))
        .route(policy(Method::POST, "/v1/peers/share"), post(share))
        .route(policy(Method::POST, "/v1/peers/forget"), post(forget))
        .route(policy(Method::POST, "/v1/peers/policy"), post(trust_policy))
        .route(
            policy(Method::POST, "/v1/peers/attachment"),
            post(attachment),
        )
        .route(policy(Method::POST, "/v1/peers/read"), post(mark_read))
}
async fn discover(
    State(state): State<PeerConnectionApiState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> ApiResult {
    service(&state, &principal).map_err(error)?;
    let workshops = crate::peer_completion_delivery::completion_execution_service()
        .run(
            medousa_forge::execution::ExecutionClass::Observation,
            medousa_forge::execution::MAX_STORE_PAYLOAD_BYTES,
            || {
                Ok(crate::pairing::mdns::browse_workshops(
                    std::time::Duration::from_secs(3),
                ))
            },
        )
        .await
        .map_err(|e| error(e.into()))?
        .map_err(error)?;
    Ok(Json(json!({"workshops":workshops})))
}
async fn projection(service: Arc<PairingService>) -> Result<Value> {
    let outbound = connections::list(service.clone()).await?;
    let status = service.pair_status().await?;
    let mut peers: Vec<Value> = outbound.iter().map(|p| json!({"id":p.workshop_id,"device":p.workshop_device_id,"pairing":p.pairing_id,"name":p.label,"inbound":false,"ready":p.has_session_token})).collect();
    for p in &status.paired_devices {
        peers.push(json!({"id":format!("inbound:{}",p.pairing_id),"device":p.phone_id,"pairing":p.pairing_id,"name":p.phone_name,"inbound":true,"trustActive":p.trust_active,"trustExpiresAt":p.trust_expires_at,"idleTimeoutSeconds":p.idle_timeout_seconds,"role":p.role,"ready":outbound.iter().any(|r|r.workshop_device_id==p.phone_id)}));
    }
    Ok(json!({"peers":peers,"connections":outbound,"pairStatus":status}))
}
async fn status(
    State(state): State<PeerConnectionApiState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> ApiResult {
    Ok(Json(
        projection(service(&state, &principal).map_err(error)?)
            .await
            .map_err(error)?,
    ))
}
async fn snapshot(
    State(state): State<PeerConnectionApiState>,
    Extension(principal): Extension<RequestPrincipal>,
) -> ApiResult {
    let service = service(&state, &principal).map_err(error)?;
    let mut peers: Vec<Value> = connections::list(service.clone())
        .await
        .map_err(error)?
        .iter()
        .map(|p| serde_json::to_value(p).unwrap_or_default())
        .collect();
    for peer in service.pair_status().await.map_err(error)?.paired_devices {
        if !peers.iter().any(|p| p["workshopDeviceId"] == peer.phone_id) {
            peers.push(json!({"workshopId":format!("inbound:{}",peer.pairing_id),"workshopDeviceId":peer.phone_id,"label":peer.phone_name,"inbound":true,"hasSessionToken":false,"hasIrohTicket":false}));
        }
    }
    let messages = connections::store_io(|| {
        crate::peer_messages::list_messages_filtered(false, None)?
            .into_iter()
            .map(|m| {
                let mut v = serde_json::to_value(m)?;
                v["sinkKind"] = json!("host");
                Ok(v)
            })
            .collect::<Result<Vec<Value>>>()
    })
    .await
    .map_err(error)?;
    Ok(Json(json!({"peers":peers,"messages":messages,"errors":[]})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairCommand {
    invite: String,
    url: String,
    name: String,
}
async fn pair(
    State(state): State<PeerConnectionApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(command): Json<PairCommand>,
) -> ApiResult {
    let service = service(&state, &principal).map_err(error)?;
    if !command.invite.trim().starts_with("medousa://") {
        return Err(error(anyhow::anyhow!(
            "Paste an authenticated Medousa pairing link; short codes require explicit expected device identity"
        )));
    }
    let _ = command.name; // Display authority comes from authenticated remote identity, never UI copy.
    let summary = connections::pair(
        service,
        connections::PairRequest {
            qr_url: Some(command.invite),
            daemon_url: command.url,
            short_code: None,
            expected_device_id: None,
        },
    )
    .await
    .map_err(error)?;
    Ok(Json(
        serde_json::to_value(summary).map_err(|e| error(e.into()))?,
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SendCommand {
    workshop_id: String,
    body: String,
}
async fn deliver(
    service: Arc<PairingService>,
    device: String,
    body: String,
    attachment: Option<crate::share::bundle::ShareBundle>,
    kind: Option<String>,
) -> Result<Value> {
    if body.trim().is_empty() {
        bail!("message body required");
    }
    let summary = connections::list(service.clone())
        .await?
        .into_iter()
        .find(|r| r.workshop_device_id == device)
        .context("peer is inbound-only; pair this daemon to the destination before sending")?;
    let request: crate::peer_messages::PeerMessagePostRequest = serde_json::from_value(
        json!({"body":body,"fromDeviceId":service.device_id(),"fromName":service.peer_name(),"toDeviceId":device,"toName":summary.label,"attachment":attachment,"kind":kind}),
    )?;
    let payload = serde_json::to_value(&request)?;
    let acknowledgement = connections::message(service.clone(), &device, payload).await?;
    // Preserve a local outbound copy only after the peer acknowledged acceptance.
    // A lost acknowledgement is explicitly unknown and is never retried automatically.
    let from = service.device_id().to_string();
    let name = service.peer_name().to_string();
    connections::store_io(move || {
        let message = crate::peer_messages::build_message(
            request,
            &from,
            &name,
            "out",
            Some(&device),
            Some(&summary.label),
        )?;
        crate::peer_messages::append_message(message)?;
        Ok(())
    })
    .await?;
    Ok(acknowledgement)
}
async fn send(
    State(state): State<PeerConnectionApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(command): Json<SendCommand>,
) -> ApiResult {
    Ok(Json(
        deliver(
            service(&state, &principal).map_err(error)?,
            command.workshop_id,
            command.body,
            None,
            None,
        )
        .await
        .map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShareCommand {
    id: String,
    device: Option<String>,
    pairing: Option<String>,
    item: String,
    artifact: bool,
    mode: String,
}
async fn share(
    State(state): State<PeerConnectionApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(command): Json<ShareCommand>,
) -> ApiResult {
    let service = service(&state, &principal).map_err(error)?;
    let rows = connections::list(service.clone()).await.map_err(error)?;
    let peer = rows
        .iter()
        .find(|r| r.workshop_id == command.id)
        .context("outbound peer not found")
        .map_err(error)?;
    if command
        .device
        .as_ref()
        .is_some_and(|d| d != &peer.workshop_device_id)
        || command
            .pairing
            .as_ref()
            .is_some_and(|p| p != &peer.pairing_id)
    {
        return Err(error(anyhow::anyhow!(
            "peer identity changed; refresh before sharing"
        )));
    }
    if command.item.trim().is_empty() || !matches!(command.mode.as_str(), "share" | "ask" | "bring")
    {
        return Err(error(anyhow::anyhow!("choose one item and share mode")));
    }
    let source = crate::share::bundle::ShareSourceWorkshop {
        device_id: service.device_id().into(),
        name: service.peer_name().into(),
    };
    let item = command.item.clone();
    let artifact = command.artifact;
    let bundle = connections::store_io(move || {
        if artifact {
            crate::share::service::export_single_artifact(&item, source)
        } else {
            crate::share::service::export_single_vault_note(&item, source)
        }
    })
    .await
    .map_err(error)?;
    let kind = match command.mode.as_str() {
        "ask" => Some("review_request".into()),
        "bring" => Some("bring_home".into()),
        _ => None,
    };
    Ok(Json(
        deliver(
            service,
            peer.workshop_device_id.clone(),
            format!(
                "{} {} `{}`",
                command.mode,
                if artifact { "artifact" } else { "note" },
                command.item
            ),
            Some(bundle),
            kind,
        )
        .await
        .map_err(error)?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ForgetCommand {
    id: String,
    device: String,
    pairing: String,
}
async fn forget(
    State(state): State<PeerConnectionApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(command): Json<ForgetCommand>,
) -> ApiResult {
    let service = service(&state, &principal).map_err(error)?;
    if command.id == format!("inbound:{}", command.pairing) {
        let records = service.pair_status().await.map_err(error)?.paired_devices;
        if !records
            .iter()
            .any(|p| p.pairing_id == command.pairing && p.phone_id == command.device)
        {
            return Err(error(anyhow::anyhow!(
                "pairing identity changed; refresh first"
            )));
        }
        service
            .revoke_pairing(
                &command.pairing,
                crate::pairing::RevokePairingAuthority::Administrator,
            )
            .await
            .map_err(error)?;
    } else {
        if command.id != command.device {
            return Err(error(anyhow::anyhow!("outbound identity mismatch")));
        }
        connections::forget(service, command.device, command.pairing)
            .await
            .map_err(error)?;
    }
    Ok(Json(json!({"removed":true})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyCommand {
    device: String,
    pairing: String,
    expires: Option<chrono::DateTime<chrono::Utc>>,
    idle: Option<u64>,
}
async fn trust_policy(
    State(state): State<PeerConnectionApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(command): Json<PolicyCommand>,
) -> ApiResult {
    let service = service(&state, &principal).map_err(error)?;
    let records = service.pair_status().await.map_err(error)?.paired_devices;
    if !records
        .iter()
        .any(|p| p.pairing_id == command.pairing && p.phone_id == command.device)
    {
        return Err(error(anyhow::anyhow!(
            "inbound pairing identity changed; refresh first"
        )));
    }
    let value = service
        .update_trust_policy(
            &command.pairing,
            crate::pairing::PairTrustPolicyUpdateRequest {
                trust_expires_at: command.expires,
                idle_timeout_seconds: command.idle,
            },
        )
        .await
        .map_err(error)?
        .context("pairing removed")
        .map_err(error)?;
    Ok(Json(
        serde_json::to_value(value).map_err(|e| error(e.into()))?,
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessageSelector {
    message_id: String,
    sink_kind: String,
    #[serde(default)]
    workshop_id: String,
}
fn local_selector(selector: &MessageSelector) -> Result<()> {
    if selector.message_id.trim().is_empty()
        || selector.sink_kind != "host"
        || !selector.workshop_id.is_empty()
    {
        bail!("unknown message source; refresh daemon snapshot");
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AttachmentCommand {
    message_id: String,
    sink_kind: String,
    #[serde(default)]
    workshop_id: String,
    item: String,
    artifact: bool,
}
fn authorize_attachment(
    message: &crate::peer_messages::PeerMessage,
    item: &str,
    artifact: bool,
) -> Result<()> {
    let result = message
        .attachment_result
        .as_ref()
        .filter(|r| r.imported)
        .context("attachment has no recipient-local imported locator")?;
    if if artifact {
        !result.artifact_ids.iter().any(|id| id == item)
    } else {
        !result.vault_paths.iter().any(|p| p == item)
    } {
        bail!("attachment is not part of this message");
    }
    Ok(())
}
async fn attachment(
    State(state): State<PeerConnectionApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(command): Json<AttachmentCommand>,
) -> ApiResult {
    service(&state, &principal).map_err(error)?;
    local_selector(&MessageSelector {
        message_id: command.message_id.clone(),
        sink_kind: command.sink_kind,
        workshop_id: command.workshop_id,
    })
    .map_err(error)?;
    let value=connections::store_io(move || {
        let message=crate::peer_messages::get_message(&command.message_id)?.context("message not found")?;
        authorize_attachment(&message,&command.item,command.artifact)?;
        // Export gives a bounded safe text response. No rendering/execution of HTML or scripts.
        let source=crate::share::bundle::ShareSourceWorkshop{device_id:"preview".into(),name:"Preview".into()};
        if command.artifact {
            let bundle=crate::share::service::export_single_artifact(&command.item,source)?;
            let entry=bundle.artifacts.into_iter().find(|e|e.id==command.item).context("exact imported artifact not found")?;
            use base64::Engine;
            let content=String::from_utf8(base64::engine::general_purpose::STANDARD.decode(entry.content_base64)?).context("preview is not UTF-8 text")?;
            Ok(json!({"title":entry.title,"content":content,"mime":entry.mime,"artifact":true,"id":entry.id}))
        } else {
            let bundle=crate::share::service::export_single_vault_note(&command.item,source)?;
            let entry=bundle.vault_notes.into_iter().find(|e|e.path==command.item).context("exact imported note not found")?;
            Ok(json!({"title":entry.path,"content":entry.content,"mime":"text/markdown","artifact":false}))
        }
    }).await.map_err(error)?;
    Ok(Json(value))
}
async fn mark_read(
    State(state): State<PeerConnectionApiState>,
    Extension(principal): Extension<RequestPrincipal>,
    Json(selector): Json<MessageSelector>,
) -> ApiResult {
    service(&state, &principal).map_err(error)?;
    local_selector(&selector).map_err(error)?;
    let value = connections::store_io(move || {
        Ok(serde_json::to_value(crate::peer_messages::mark_read(
            &selector.message_id,
        )?)?)
    })
    .await
    .map_err(error)?;
    Ok(Json(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_connection_routes_are_native_admin_only() {
        let routes = surface();
        let inventory = routes.inventory();
        assert_eq!(inventory.entries().len(), 10);
        for entry in inventory.entries() {
            assert_eq!(
                entry.required_capability,
                Some(Capability::AdminIdentity.as_str())
            );
            assert!(!entry.bootstrap_public);
            assert_eq!(entry.browser_policy, BrowserPolicy::NativeOnly);
        }
    }
    #[test]
    fn peer_and_anonymous_credentials_cannot_operate_daemon_connections() {
        use crate::request_principal::TransportClass;
        assert!(owner(&RequestPrincipal::anonymous(TransportClass::Loopback)).is_err());
        assert!(owner(&RequestPrincipal::peer_for_test(None)).is_err());
        assert!(
            owner(&RequestPrincipal::local_app(
                std::sync::Arc::from("owner"),
                TransportClass::Loopback
            ))
            .is_ok()
        );
    }
    #[test]
    fn source_selector_cannot_redirect_to_another_workshop() {
        assert!(
            local_selector(&MessageSelector {
                message_id: "m".into(),
                sink_kind: "host".into(),
                workshop_id: "".into()
            })
            .is_ok()
        );
        for (kind, workshop) in [("peer", "remote"), ("unknown", ""), ("host", "forged")] {
            assert!(
                local_selector(&MessageSelector {
                    message_id: "m".into(),
                    sink_kind: kind.into(),
                    workshop_id: workshop.into()
                })
                .is_err()
            );
        }
    }
}
