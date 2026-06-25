use crate::daemon::sdk;
use crate::daemon::sse::stream_sse_json_workshop;
use crate::daemon::workshop_http;
use crate::daemon::DaemonState;
use crate::local_engine;
use crate::workshop_registry::{load_registry, PERSONAL_WORKSHOP_ID};
use medousa_types::{
    LocalCatalogResponse, LocalEngineStatus, LocalHardwareResponse, LocalModelsResponse,
    ModelDownloadProgress,
};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::watch;

pub struct LocalInferenceStreamState {
    cancel: Mutex<Option<watch::Sender<bool>>>,
}

impl LocalInferenceStreamState {
    pub fn new() -> Self {
        Self {
            cancel: Mutex::new(None),
        }
    }
}

#[tauri::command]
pub async fn local_inference_hardware(
    state: State<'_, DaemonState>,
) -> Result<LocalHardwareResponse, String> {
    sdk::client(&state)
        .local_models()
        .hardware()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn local_inference_catalog(
    state: State<'_, DaemonState>,
) -> Result<LocalCatalogResponse, String> {
    sdk::client(&state)
        .local_models()
        .catalog()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn local_inference_models(
    state: State<'_, DaemonState>,
) -> Result<LocalModelsResponse, String> {
    sdk::client(&state)
        .local_models()
        .list()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn local_inference_start_download(
    state: State<'_, DaemonState>,
    model_id: String,
) -> Result<ModelDownloadProgress, String> {
    sdk::client(&state)
        .local_models()
        .start_download(model_id.trim())
        .await
        .map(|response| response.job)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn local_inference_download_status(
    state: State<'_, DaemonState>,
    job_id: String,
) -> Result<ModelDownloadProgress, String> {
    workshop_http::get_json(
        &state,
        &format!("/v1/local/models/download/{}", job_id.trim()),
    )
    .await
}

/// Spawn `medousa_local` on the desktop (daemon only probes engine status).
#[tauri::command]
pub async fn local_inference_spawn_engine(
    state: State<'_, DaemonState>,
    model_id: Option<String>,
) -> Result<LocalEngineStatus, String> {
    let registry = load_registry()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|entry| entry.id == PERSONAL_WORKSHOP_ID)
        .ok_or_else(|| "personal workshop not found in registry".to_string())?;
    let data_dir = local_engine::resolve_workshop_data_dir(workshop);
    let model = model_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    local_engine::ensure_local_brain(&workshop.id, &data_dir, model.as_deref()).await?;

    sdk::client(&state)
        .local_models()
        .engine_status()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn local_inference_engine_status(
    state: State<'_, DaemonState>,
) -> Result<LocalEngineStatus, String> {
    sdk::client(&state)
        .local_models()
        .engine_status()
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn local_inference_remove_model(
    state: State<'_, DaemonState>,
    model_id: String,
) -> Result<serde_json::Value, String> {
    sdk::client(&state)
        .local_models()
        .remove_model(model_id.trim())
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn local_inference_stream_download(
    app: AppHandle,
    state: State<'_, DaemonState>,
    stream_state: State<'_, LocalInferenceStreamState>,
    job_id: String,
) -> Result<(), String> {
    if let Some(cancel) = stream_state.cancel.lock().expect("lock").take() {
        let _ = cancel.send(true);
    }
    let (cancel_tx, cancel_rx) = watch::channel(false);
    *stream_state.cancel.lock().expect("lock") = Some(cancel_tx);

    let config = workshop_http::transport_config(&state);
    let path = format!("/v1/local/models/download/{}/events", job_id.trim());

    tauri::async_runtime::spawn(async move {
        match workshop_http::get_bytes_stream_for_config(&config, &path).await {
            Ok(source) => {
                stream_sse_json_workshop::<ModelDownloadProgress, _>(
                    &app,
                    source,
                    "model_download_progress",
                    "model_download_progress://error",
                    |_| {},
                    cancel_rx,
                )
                .await;
            }
            Err(err) => {
                let _ = app.emit(
                    "model_download_progress://error",
                    serde_json::json!({ "message": err }),
                );
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn local_inference_stream_download_stop(
    stream_state: State<'_, LocalInferenceStreamState>,
) -> Result<(), String> {
    if let Some(cancel) = stream_state.cancel.lock().expect("lock").take() {
        let _ = cancel.send(true);
    }
    Ok(())
}
