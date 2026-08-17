use crate::daemon::sdk;
use crate::daemon::sse::stream_sse_json_workshop;
use crate::daemon::workshop_http;
use crate::daemon::DaemonState;
use crate::workshop_registry::{load_registry, PERSONAL_WORKSHOP_ID};
use crate::workshop_runtime;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use medousa_types::LocalResourceAdmission;
use medousa_types::{
    LocalCatalogResponse, LocalEngineStatus, LocalHardwareResponse, LocalModelsResponse,
    LocalRuntimePhase, ModelDownloadProgress,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::watch;

pub struct LocalInferenceStreamState {
    cancel: Mutex<Option<watch::Sender<bool>>>,
}

pub struct LocalInferenceActivationState {
    next_id: AtomicU64,
    pending: Mutex<HashMap<String, (u64, watch::Sender<bool>)>>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn require_local_resource_admission(
    model_id: Option<&str>,
) -> Result<LocalResourceAdmission, String> {
    let admission = match model_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(model_id) => medousa_local_inference::admission_for_model_id(model_id)?,
        None => medousa_local_inference::recommended_model_admission()?,
    };
    if admission.admitted {
        Ok(admission)
    } else {
        Err(admission.rationale)
    }
}

impl LocalInferenceStreamState {
    pub fn new() -> Self {
        Self {
            cancel: Mutex::new(None),
        }
    }
}

impl LocalInferenceActivationState {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
        }
    }

    fn begin(&self, session_id: &str) -> (u64, watch::Receiver<bool>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = watch::channel(false);
        if let Some((_, previous)) = self
            .pending
            .lock()
            .expect("local activation lock")
            .insert(session_id.to_string(), (id, tx))
        {
            let _ = previous.send(true);
        }
        (id, rx)
    }

    fn finish(&self, session_id: &str, id: u64) {
        let mut pending = self.pending.lock().expect("local activation lock");
        if pending
            .get(session_id)
            .is_some_and(|(active_id, _)| *active_id == id)
        {
            pending.remove(session_id);
        }
    }

    pub fn cancel(&self, session_id: &str) -> bool {
        self.pending
            .lock()
            .expect("local activation lock")
            .get(session_id)
            .is_some_and(|(_, cancel)| cancel.send(true).is_ok())
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
    sdk::client(&state)
        .local_models()
        .download_status(job_id.trim())
        .await
        .map_err(|err| err.to_string())
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
    let data_dir = workshop_runtime::resolve_workshop_data_dir(workshop);
    let model = model_id
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _admission = require_local_resource_admission(model.as_deref())?;

    workshop_runtime::ensure_local_brain(&workshop.id, &data_dir, model.as_deref()).await?;

    sdk::client(&state)
        .local_models()
        .engine_status()
        .await
        .map_err(|err| err.to_string())
}

pub(crate) async fn ensure_local_engine_for_turn(
    activation_state: &LocalInferenceActivationState,
    session_id: &str,
    model_id: Option<&str>,
) -> Result<(), String> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    let _admission = require_local_resource_admission(model_id)?;
    let registry = load_registry()?;
    let workshop = registry
        .workshops
        .iter()
        .find(|entry| entry.id == PERSONAL_WORKSHOP_ID)
        .ok_or_else(|| "personal workshop not found in registry".to_string())?;
    let data_dir = workshop_runtime::resolve_workshop_data_dir(workshop);
    let model_id = model_id.map(str::trim).filter(|value| !value.is_empty());
    let (activation_id, mut cancel) = activation_state.begin(session_id);
    let activation = workshop_runtime::ensure_local_brain(&workshop.id, &data_dir, model_id);
    tokio::pin!(activation);
    let result = tokio::select! {
        biased;
        changed = cancel.changed() => {
            if changed.is_ok() && *cancel.borrow() {
                workshop_runtime::stop_local_brain_bounded(&workshop.id)
                    .await
                    .map(|_| ())
                    .and_then(|_| Err("Local model loading was cancelled".to_string()))
            } else {
                activation.await.and_then(|ready| {
                    if ready {
                        Ok(())
                    } else {
                        Err("Offline brain package is not installed".to_string())
                    }
                })
            }
        }
        ready = &mut activation => ready.and_then(|ready| {
            if ready {
                Ok(())
            } else {
                Err("Offline brain package is not installed".to_string())
            }
        }),
    };
    activation_state.finish(session_id, activation_id);
    result
}

#[tauri::command]
pub async fn local_inference_unload_engine(
    state: State<'_, DaemonState>,
) -> Result<LocalEngineStatus, String> {
    workshop_runtime::stop_local_brain_bounded(PERSONAL_WORKSHOP_ID).await?;
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
    let mut status = sdk::client(&state)
        .local_models()
        .engine_status()
        .await
        .map_err(|err| err.to_string())?;
    if !status.loaded
        && matches!(
            status.phase,
            LocalRuntimePhase::Cold | LocalRuntimePhase::Unavailable
        )
        && workshop_runtime::local_brain_process_alive(PERSONAL_WORKSHOP_ID)
    {
        status.phase = LocalRuntimePhase::Loading;
        status.message = "Local model is loading".to_string();
    }
    Ok(status)
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
    let path = medousa_sdk::generated::expand_path(
        medousa_sdk::generated::ops::LOCAL_MODELS_DOWNLOAD_BY_JOB_ID_EVENTS_GET.path,
        &[("job_id", job_id.trim())],
    )?;

    tauri::async_runtime::spawn(async move {
        match workshop_http::get_bytes_stream_for_config(&config, &path).await {
            Ok(source) => {
                stream_sse_json_workshop::<ModelDownloadProgress>(
                    &app,
                    source,
                    "model_download_progress",
                    "model_download_progress://error",
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

#[cfg(test)]
mod tests {
    use super::LocalInferenceActivationState;

    #[test]
    fn pending_activation_can_be_cancelled_before_turn_creation() {
        let state = LocalInferenceActivationState::new();
        let (_id, cancel) = state.begin("session-1");
        assert!(state.cancel("session-1"));
        assert!(*cancel.borrow());
    }

    #[test]
    fn finishing_an_old_activation_does_not_remove_its_replacement() {
        let state = LocalInferenceActivationState::new();
        let (old_id, old_cancel) = state.begin("session-1");
        let (_new_id, new_cancel) = state.begin("session-1");
        assert!(*old_cancel.borrow());
        state.finish("session-1", old_id);
        assert!(state.cancel("session-1"));
        assert!(*new_cancel.borrow());
    }
}
