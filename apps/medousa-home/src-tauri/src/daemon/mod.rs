pub mod artifact;
pub mod catalog;
pub mod identity;
pub mod jobs;
pub mod recurring;
pub mod runtime;
pub mod session;
pub mod sse;
pub mod types;
pub mod vault;
pub mod workspace_card;

use crate::daemon::sse::stream_sse_json;
use crate::daemon::types::{
    DaemonHealth, HealthResponse, InteractiveTurnAccepted, InteractiveTurnRequest,
    InteractiveTurnResponse, InteractiveTurnStreamEvent, StageRoutingMatrix,
    TurnSurfaceContext, WorkspaceStreamEvent, DEFAULT_DAEMON_URL,
};
use reqwest::Client;
use std::sync::Mutex;
use tauri::{AppHandle, State};
use tokio::sync::watch;

pub struct DaemonState {
    pub daemon_url: Mutex<String>,
    workspace_cancel: Mutex<Option<watch::Sender<bool>>>,
    interactive_cancel: Mutex<Option<watch::Sender<bool>>>,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            daemon_url: Mutex::new(resolve_daemon_url()),
            workspace_cancel: Mutex::new(None),
            interactive_cancel: Mutex::new(None),
        }
    }
}

pub fn resolve_daemon_url() -> String {
    std::env::var("MEDOUSA_DAEMON_URL")
        .or_else(|_| std::env::var("STASIS_DAEMON_URL"))
        .unwrap_or_else(|_| DEFAULT_DAEMON_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn turn_defaults() -> (String, String) {
    let provider = std::env::var("MEDOUSA_HOME_PROVIDER").unwrap_or_else(|_| "ollama".to_string());
    let model = std::env::var("MEDOUSA_HOME_MODEL").unwrap_or_else(|_| "qwen2.5:7b".to_string());
    (provider, model)
}

fn replace_cancel_slot(slot: &Mutex<Option<watch::Sender<bool>>>) -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);
    if let Some(previous) = slot.lock().expect("daemon cancel lock").take() {
        let _ = previous.send(true);
    }
    slot.lock().expect("daemon cancel lock").replace(tx);
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
    *state.daemon_url.lock().expect("daemon url lock") = trimmed;
    Ok(())
}

#[tauri::command]
pub async fn daemon_health(state: State<'_, DaemonState>) -> Result<DaemonHealth, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::new();
    let url = format!("{base}/health");
    match client.get(&url).send().await {
        Ok(response) if response.status().is_success() => {
            let detail = response.json::<HealthResponse>().await.ok();
            let message = detail
                .as_ref()
                .map(|health| {
                    format!(
                        "connected to {base} · {} tools",
                        health.tool_registry_count
                    )
                })
                .unwrap_or_else(|| format!("connected to {base}"));
            Ok(DaemonHealth {
                ok: true,
                message,
                backend: detail.as_ref().map(|h| h.backend.clone()),
                worker_id: detail.as_ref().map(|h| h.worker_id.clone()),
                tool_registry_count: detail.map(|h| h.tool_registry_count),
            })
        }
        Ok(response) => Ok(DaemonHealth {
            ok: false,
            message: format!("daemon returned HTTP {}", response.status()),
            backend: None,
            worker_id: None,
            tool_registry_count: None,
        }),
        Err(err) => Ok(DaemonHealth {
            ok: false,
            message: format!("cannot reach {base}: {err}"),
            backend: None,
            worker_id: None,
            tool_registry_count: None,
        }),
    }
}

#[tauri::command]
pub async fn workspace_stream_start(
    app: AppHandle,
    state: State<'_, DaemonState>,
    since_revision: Option<u64>,
) -> Result<(), String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let mut url = format!("{base}/v1/workspace/stream");
    if let Some(revision) = since_revision {
        url.push_str(&format!("?since_revision={revision}"));
    }

    let cancel_rx = replace_cancel_slot(&state.workspace_cancel);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|err| err.to_string())?;

    tokio::spawn(async move {
        stream_sse_json::<WorkspaceStreamEvent, _>(
            &app,
            &client,
            &url,
            "workspace://event",
            "workspace://error",
            |_event| {},
            cancel_rx,
        )
        .await;
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

#[tauri::command]
pub async fn interactive_turn_send(
    state: State<'_, DaemonState>,
    session_id: String,
    prompt: String,
    provider: Option<String>,
    model: Option<String>,
    response_depth_mode: Option<String>,
    stage_routing: Option<StageRoutingMatrix>,
) -> Result<InteractiveTurnAccepted, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let (default_provider, default_model) = turn_defaults();
    let provider = provider
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or(default_provider);
    let model = model
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or(default_model);
    let response_depth_mode = response_depth_mode
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "standard".to_string());
    let stage_routing =
        stage_routing.unwrap_or_else(|| StageRoutingMatrix::default_for(&provider, &model));

    let request = InteractiveTurnRequest {
        session_id,
        prompt,
        persist_user_turn: true,
        response_depth_mode,
        provider: provider.clone(),
        model: model.clone(),
        stage_routing,
        surface: Some(TurnSurfaceContext {
            channel_surface: Some("home".to_string()),
            channel_id: None,
            user_id: None,
        }),
    };

    let client = Client::new();
    let response = client
        .post(format!("{base}/v1/interactive/turn"))
        .json(&request)
        .send()
        .await
        .map_err(|err| err.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("interactive turn failed ({status}): {body}"));
    }

    let parsed: InteractiveTurnResponse = response.json().await.map_err(|err| err.to_string())?;
    Ok(InteractiveTurnAccepted {
        turn_id: parsed.turn_id,
        stream_url: parsed.stream_url,
    })
}

#[tauri::command]
pub async fn interactive_stream_start(
    app: AppHandle,
    state: State<'_, DaemonState>,
    stream_url: String,
) -> Result<(), String> {
    let cancel_rx = replace_cancel_slot(&state.interactive_cancel);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|err| err.to_string())?;

    tokio::spawn(async move {
        stream_sse_json::<InteractiveTurnStreamEvent, _>(
            &app,
            &client,
            &stream_url,
            "interactive://event",
            "interactive://error",
            |_event| {},
            cancel_rx,
        )
        .await;
    });

    Ok(())
}

#[tauri::command]
pub fn interactive_stream_stop(state: State<'_, DaemonState>) -> Result<(), String> {
    if let Some(tx) = state
        .interactive_cancel
        .lock()
        .expect("interactive cancel lock")
        .take()
    {
        let _ = tx.send(true);
    }
    Ok(())
}
