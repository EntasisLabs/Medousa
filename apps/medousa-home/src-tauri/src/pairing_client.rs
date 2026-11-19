//! Platform heartbeat adapter; pairing credentials and signing are shared.
pub use medousa_desktop_host::pairing_client::*;

pub async fn send_pair_heartbeat(
    lan_base: &str,
    body: Option<&crate::pairing::PairHeartbeatInvokeRequest>,
) -> Result<(), String> {
    let Some(config) = load_workshop_transport_config(lan_base) else {
        return Ok(());
    };

    let apns_token = if body.is_some_and(|body| body.remote_push_enabled == Some(false)) {
        None
    } else {
        body.and_then(|body| body.apns_device_token.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(crate::push::current_apns_device_token)
    };

    let live_activity_token = body
        .and_then(|body| body.live_activity_push_token.as_ref())
        .map(|value| value.trim().to_string());

    let mesh_lan = body
        .and_then(|body| body.mesh_lan_base_url.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            crate::workshop_runtime::lan_pairing_status()
                .ok()
                .filter(|status| status.enabled)
                .map(|status| status.url.trim().trim_end_matches('/').to_string())
                .filter(|value| !value.is_empty())
        });
    let mesh_ticket = body
        .and_then(|body| body.mesh_iroh_ticket.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mesh_endpoint = body
        .and_then(|body| body.mesh_iroh_endpoint_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let has_push = apns_token.is_some()
        || body
            .and_then(|body| body.push_platform.as_deref())
            .is_some()
        || live_activity_token.is_some()
        || body.is_some_and(|body| {
            body.remote_push_enabled.is_some()
                || body.turn_updates_enabled.is_some()
                || body.needs_input_enabled.is_some()
                || body.peer_messages_enabled.is_some()
                || body.reminders_enabled.is_some()
        });
    let has_mesh = mesh_lan.is_some() || mesh_ticket.is_some() || mesh_endpoint.is_some();

    if has_push || has_mesh {
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
        if body.is_some_and(|body| body.remote_push_enabled == Some(false)) {
            payload.insert(
                "apnsDeviceToken".into(),
                serde_json::Value::String(String::new()),
            );
        }
        if let Some(body) = body {
            for (key, value) in [
                ("remotePushEnabled", body.remote_push_enabled),
                ("turnUpdatesEnabled", body.turn_updates_enabled),
                ("needsInputEnabled", body.needs_input_enabled),
                ("peerMessagesEnabled", body.peer_messages_enabled),
                ("remindersEnabled", body.reminders_enabled),
            ] {
                if let Some(value) = value {
                    payload.insert(key.into(), serde_json::Value::Bool(value));
                }
            }
        }
        if let Some(token) = live_activity_token {
            payload.insert(
                "liveActivityPushToken".into(),
                serde_json::Value::String(token),
            );
        }
        if let Some(url) = mesh_lan {
            payload.insert("meshLanBaseUrl".into(), serde_json::Value::String(url));
        }
        if let Some(ticket) = mesh_ticket {
            payload.insert("meshIrohTicket".into(), serde_json::Value::String(ticket));
        }
        if let Some(endpoint) = mesh_endpoint {
            payload.insert(
                "meshIrohEndpointId".into(),
                serde_json::Value::String(endpoint),
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
