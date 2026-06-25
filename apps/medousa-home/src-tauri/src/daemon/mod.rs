pub mod artifact;
pub mod catalog;
pub mod grapheme;
pub mod identity;
pub mod jobs;
pub mod local_inference;
pub mod locus;
pub mod media;
pub mod model_catalog;
pub mod recurring;
pub mod runtime;
pub mod sdk;
pub mod session;
pub mod sse;
pub mod stt;
pub mod types;
pub mod vault;
pub mod workspace_card;
pub mod turn_budget;
pub mod tool_history;
pub mod workflow;
pub mod workshop_http;

use crate::daemon::sse::stream_sse_json_workshop;
use crate::daemon::types::{
    DaemonHealth, HealthResponse, InteractiveTurnAccepted, InteractiveTurnRequest,
    InteractiveTurnResponse, InteractiveTurnStreamEvent, StageRoutingMatrix,
    TurnSurfaceContext, WorkspaceStreamEvent, DEFAULT_DAEMON_URL,
};
use crate::workshop_transport;
use reqwest::Client;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::watch;

pub struct DaemonState {
    pub daemon_url: Mutex<String>,
    workspace_cancel: Mutex<Option<watch::Sender<bool>>>,
    /// One SSE listener per turn id — Tier 2c multi-stream bridge.
    interactive_streams: Mutex<HashMap<String, watch::Sender<bool>>>,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            daemon_url: Mutex::new(resolve_daemon_url()),
            workspace_cancel: Mutex::new(None),
            interactive_streams: Mutex::new(HashMap::new()),
        }
    }
}

pub fn resolve_daemon_url() -> String {
    std::env::var("MEDOUSA_DAEMON_URL")
        .or_else(|_| std::env::var("STASIS_DAEMON_URL"))
        .ok()
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .or_else(read_persisted_daemon_url)
        .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_string())
}

fn daemon_url_store_path() -> PathBuf {
    crate::paths::medousa_data_dir().join("home_daemon_url.txt")
}

fn read_persisted_daemon_url() -> Option<String> {
    let raw = std::fs::read_to_string(daemon_url_store_path()).ok()?;
    let trimmed = raw.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn persist_daemon_url(url: &str) -> Result<(), String> {
    let path = daemon_url_store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    std::fs::write(path, url).map_err(|err| err.to_string())
}

pub(crate) fn daemon_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|err| err.to_string())
}

/// Replace bind-only hosts (0.0.0.0, loopback) with the client-configured daemon URL.
fn rewrite_stream_url_for_client(stream_url: &str, daemon_url: &str) -> String {
    const UNROUTABLE: [&str; 4] = ["0.0.0.0", "127.0.0.1", "localhost", "[::1]"];

    let Some(after_scheme) = stream_url.split("://").nth(1) else {
        return stream_url.to_string();
    };
    let host_end = after_scheme.find('/').unwrap_or(after_scheme.len());
    let host_port = &after_scheme[..host_end];
    let host = host_port.split(':').next().unwrap_or(host_port);
    if !UNROUTABLE.contains(&host) {
        return stream_url.to_string();
    }

    let path = if host_end < after_scheme.len() {
        &after_scheme[host_end..]
    } else {
        ""
    };
    let base = daemon_url.trim().trim_end_matches('/');
    format!("{base}{path}")
}

fn replace_cancel_slot(slot: &Mutex<Option<watch::Sender<bool>>>) -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);
    if let Some(previous) = slot.lock().expect("daemon cancel lock").take() {
        let _ = previous.send(true);
    }
    slot.lock().expect("daemon cancel lock").replace(tx);
    rx
}

fn extract_turn_id_from_stream_url(stream_url: &str) -> Option<String> {
    const MARKER: &str = "/v1/interactive/turn/";
    let start = stream_url.find(MARKER)? + MARKER.len();
    let rest = &stream_url[start..];
    let end = rest.find("/stream").or_else(|| rest.find('?'))?;
    let turn_id = rest[..end].trim();
    if turn_id.is_empty() {
        None
    } else {
        Some(turn_id.to_string())
    }
}

fn add_interactive_stream_slot(
    streams: &Mutex<HashMap<String, watch::Sender<bool>>>,
    turn_id: &str,
) -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);
    let mut guard = streams.lock().expect("interactive streams lock");
    if let Some(previous) = guard.remove(turn_id) {
        let _ = previous.send(true);
    }
    guard.insert(turn_id.to_string(), tx);
    rx
}

#[tauri::command]
pub fn daemon_url(state: State<'_, DaemonState>) -> String {
    state
        .daemon_url
        .lock()
        .expect("daemon url lock")
        .clone()
}

#[tauri::command]
pub fn set_daemon_url(state: State<'_, DaemonState>, url: String) -> Result<(), String> {
    let trimmed = url.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err("daemon URL cannot be empty".to_string());
    }
    apply_daemon_url(&state, &trimmed)?;
    let _ = crate::workshop_registry::update_active_workshop_url(&trimmed);
    Ok(())
}

pub fn apply_daemon_url(state: &DaemonState, url: &str) -> Result<(), String> {
    let trimmed = url.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err("daemon URL cannot be empty".to_string());
    }
    *state.daemon_url.lock().expect("daemon url lock") = trimmed.clone();
    persist_daemon_url(&trimmed)?;
    workshop_transport::invalidate_workshop_route_cache();
    Ok(())
}

#[tauri::command]
pub async fn daemon_health(state: State<'_, DaemonState>) -> Result<DaemonHealth, String> {
    let config = workshop_http::transport_config(&state);
    match workshop_transport::workshop_get_json::<HealthResponse>(&config, "/health").await {
        Ok(detail) => Ok(DaemonHealth {
            ok: true,
            message: format!(
                "connected to {} · {} tools",
                config.lan_base, detail.tool_registry_count
            ),
            backend: Some(detail.backend),
            worker_id: Some(detail.worker_id),
            tool_registry_count: Some(detail.tool_registry_count),
            agent_runtime_version: if detail.agent_runtime_version.is_empty() {
                None
            } else {
                Some(detail.agent_runtime_version)
            },
            last_agent_turn_at_utc: detail.last_agent_turn_at_utc,
            last_agent_turn_latency_ms: detail.last_agent_turn_latency_ms,
            active_profile_id: if detail.active_profile_id.is_empty() {
                None
            } else {
                Some(detail.active_profile_id)
            },
            active_profile_display_name: if detail.active_profile_display_name.is_empty() {
                None
            } else {
                Some(detail.active_profile_display_name)
            },
        }),
        Err(err) => Ok(DaemonHealth {
            ok: false,
            message: err,
            backend: None,
            worker_id: None,
            tool_registry_count: None,
            agent_runtime_version: None,
            last_agent_turn_at_utc: None,
            last_agent_turn_latency_ms: None,
            active_profile_id: None,
            active_profile_display_name: None,
        }),
    }
}

#[tauri::command]
pub async fn workspace_stream_start(
    app: AppHandle,
    state: State<'_, DaemonState>,
    since_revision: Option<u64>,
) -> Result<(), String> {
    let mut path = "/v1/workspace/stream".to_string();
    if let Some(revision) = since_revision {
        path.push_str(&format!("?since_revision={revision}"));
    }

    let config = workshop_http::transport_config(&state);
    let cancel_rx = replace_cancel_slot(&state.workspace_cancel);

    tokio::spawn(async move {
        match workshop_transport::workshop_get_bytes_stream(&config, &path).await {
            Ok(source) => {
                stream_sse_json_workshop::<WorkspaceStreamEvent, _>(
                    &app,
                    source,
                    "workspace://event",
                    "workspace://error",
                    |_event| {},
                    cancel_rx,
                )
                .await;
            }
            Err(err) => {
                let _ = app.emit(
                    "workspace://error",
                    serde_json::json!({ "message": err }),
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn workspace_stream_stop(state: State<'_, DaemonState>) -> Result<(), String> {
    if let Some(tx) = state
        .workspace_cancel
        .lock()
        .expect("workspace cancel lock")
        .take()
    {
        let _ = tx.send(true);
    }
    Ok(())
}

fn default_home_channel_surface() -> String {
    #[cfg(target_os = "ios")]
    {
        return "home-ios".to_string();
    }
    #[cfg(target_os = "android")]
    {
        return "home-android".to_string();
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
        "home-desktop".to_string()
    }
}

#[tauri::command]
pub async fn interactive_turn_send(
    state: State<'_, DaemonState>,
    session_id: String,
    prompt: String,
    provider: Option<String>,
    model: Option<String>,
    response_depth_mode: Option<String>,
    reasoning_effort: Option<String>,
    stage_routing: Option<StageRoutingMatrix>,
    channel_surface: Option<String>,
) -> Result<InteractiveTurnAccepted, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let provider = provider
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let model = model
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let response_depth_mode = response_depth_mode
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "standard".to_string());
    let reasoning_effort = reasoning_effort
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "default".to_string());
    let stage_routing = stage_routing.unwrap_or_else(|| {
        StageRoutingMatrix::default_for(
            if provider.is_empty() { "openai" } else { provider.as_str() },
            if model.is_empty() { "gpt-5.4-mini" } else { model.as_str() },
        )
    });

    let channel_surface = channel_surface
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(default_home_channel_surface);

    let request = InteractiveTurnRequest {
        session_id: session_id.clone(),
        prompt,
        persist_user_turn: true,
        response_depth_mode,
        reasoning_effort,
        provider: provider.clone(),
        model: model.clone(),
        stage_routing,
        surface: Some(TurnSurfaceContext {
            channel_surface: Some(channel_surface),
            channel_id: Some(session_id.clone()),
            user_id: None,
        }),
        max_tool_rounds: None,
        retry_runtime_max_rounds: None,
        manuscript_id: None,
        additional_manuscript_ids: None,
        suggested_capability_ids: None,
        voice_preset_id: None,
        voice_appendix: None,
        scheduled_tool_allowlist: None,
        media_refs: Vec::new(),
        identity_user_id: None,
    };

    let config = workshop_http::transport_config(&state);
    let parsed: InteractiveTurnResponse = workshop_transport::workshop_post_json(
        &config,
        "/v1/interactive/turn",
        &request,
    )
    .await?;
    Ok(InteractiveTurnAccepted {
        turn_id: parsed.turn_id,
        stream_url: rewrite_stream_url_for_client(&parsed.stream_url, &base),
    })
}

#[tauri::command]
pub async fn interactive_stream_start(
    app: AppHandle,
    state: State<'_, DaemonState>,
    stream_url: String,
) -> Result<(), String> {
    let daemon_url = state.daemon_url.lock().expect("daemon url lock").clone();
    let stream_url = rewrite_stream_url_for_client(&stream_url, &daemon_url);
    let turn_id = extract_turn_id_from_stream_url(&stream_url)
        .ok_or_else(|| "stream URL missing turn id".to_string())?;
    let cancel_rx = add_interactive_stream_slot(&state.interactive_streams, &turn_id);

    let config = workshop_http::transport_config(&state);
    let path = reqwest::Url::parse(&stream_url)
        .ok()
        .map(|url| {
            let mut path = url.path().to_string();
            if let Some(query) = url.query() {
                path.push('?');
                path.push_str(query);
            }
            path
        })
        .unwrap_or_else(|| stream_url.clone());

    tokio::spawn(async move {
        match workshop_transport::workshop_get_bytes_stream(&config, &path).await {
            Ok(source) => {
                stream_sse_json_workshop::<InteractiveTurnStreamEvent, _>(
                    &app,
                    source,
                    "interactive://event",
                    "interactive://error",
                    |_event| {},
                    cancel_rx,
                )
                .await;
            }
            Err(err) => {
                let _ = app.emit(
                    "interactive://error",
                    serde_json::json!({ "message": err }),
                );
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn interactive_stream_stop(state: State<'_, DaemonState>) -> Result<(), String> {
    let mut guard = state
        .interactive_streams
        .lock()
        .expect("interactive streams lock");
    for (_, tx) in guard.drain() {
        let _ = tx.send(true);
    }
    Ok(())
}

#[tauri::command]
pub fn interactive_stream_stop_turn(
    state: State<'_, DaemonState>,
    turn_id: String,
) -> Result<(), String> {
    let trimmed = turn_id.trim();
    if trimmed.is_empty() {
        return Err("turn_id is required".to_string());
    }
    if let Some(tx) = state
        .interactive_streams
        .lock()
        .expect("interactive streams lock")
        .remove(trimmed)
    {
        let _ = tx.send(true);
    }
    Ok(())
}
