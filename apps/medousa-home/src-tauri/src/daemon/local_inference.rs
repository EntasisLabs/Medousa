use crate::daemon::sse::stream_sse_json;
use crate::daemon::DaemonState;
use reqwest::Client;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, State};
use tokio::sync::watch;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalHardwareResponse {
    pub profile: serde_json::Value,
    pub engine_available: bool,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalCatalogResponse {
    pub tier: String,
    pub tier_label: String,
    pub family_default: String,
    pub recommended_model_id: String,
    pub models: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelsResponse {
    pub installed: Vec<serde_json::Value>,
    pub active_downloads: Vec<ModelDownloadProgress>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadProgress {
    pub job_id: String,
    pub model_id: String,
    pub phase: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub percent: f32,
    pub current_file: Option<String>,
    pub message: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEngineStatus {
    pub feature_enabled: bool,
    pub loaded: bool,
    pub base_url: String,
    pub bind: Option<String>,
    pub model_repo: Option<String>,
    pub model_alias: Option<String>,
    pub message: String,
}

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

async fn daemon_get<T: serde::de::DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .get(format!("{base}{path}"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("daemon GET {path} failed ({status}): {body}"));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
}

async fn daemon_post<T: serde::de::DeserializeOwned, B: serde::Serialize>(
    state: &State<'_, DaemonState>,
    path: &str,
    body: &B,
) -> Result<T, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|err| err.to_string())?;
    let response = client
        .post(format!("{base}{path}"))
        .json(body)
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("daemon POST {path} failed ({status}): {body}"));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
}

async fn daemon_delete<T: serde::de::DeserializeOwned>(
    state: &State<'_, DaemonState>,
    path: &str,
) -> Result<T, String> {
    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let client = Client::new();
    let response = client
        .delete(format!("{base}{path}"))
        .send()
        .await
        .map_err(|err| err.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("daemon DELETE {path} failed ({status}): {body}"));
    }
    response.json::<T>().await.map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn local_inference_hardware(
    state: State<'_, DaemonState>,
) -> Result<LocalHardwareResponse, String> {
    daemon_get(&state, "/v1/local/hardware").await
}

#[tauri::command]
pub async fn local_inference_catalog(
    state: State<'_, DaemonState>,
) -> Result<LocalCatalogResponse, String> {
    daemon_get(&state, "/v1/local/catalog").await
}

#[tauri::command]
pub async fn local_inference_models(
    state: State<'_, DaemonState>,
) -> Result<LocalModelsResponse, String> {
    daemon_get(&state, "/v1/local/models").await
}

#[tauri::command]
pub async fn local_inference_start_download(
    state: State<'_, DaemonState>,
    model_id: String,
) -> Result<ModelDownloadProgress, String> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Body {
        model_id: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Response {
        job: ModelDownloadProgress,
    }
    let response: Response = daemon_post(
        &state,
        "/v1/local/models/download",
        &Body {
            model_id: model_id.trim().to_string(),
        },
    )
    .await?;
    Ok(response.job)
}

#[tauri::command]
pub async fn local_inference_download_status(
    state: State<'_, DaemonState>,
    job_id: String,
) -> Result<ModelDownloadProgress, String> {
    daemon_get(
        &state,
        &format!("/v1/local/models/download/{}", job_id.trim()),
    )
    .await
}

#[tauri::command]
pub async fn local_inference_load_engine(
    state: State<'_, DaemonState>,
    model_id: Option<String>,
) -> Result<LocalEngineStatus, String> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Body {
        model_id: Option<String>,
    }
    daemon_post(
        &state,
        "/v1/local/engine/load",
        &Body {
            model_id: model_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        },
    )
    .await
}

#[tauri::command]
pub async fn local_inference_engine_status(
    state: State<'_, DaemonState>,
) -> Result<LocalEngineStatus, String> {
    daemon_get(&state, "/v1/local/engine/status").await
}

#[tauri::command]
pub async fn local_inference_remove_model(
    state: State<'_, DaemonState>,
    model_id: String,
) -> Result<serde_json::Value, String> {
    daemon_delete(
        &state,
        &format!("/v1/local/models/{}", model_id.trim()),
    )
    .await
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

    let base = state.daemon_url.lock().expect("daemon url lock").clone();
    let url = format!("{base}/v1/local/models/download/{}/events", job_id.trim());
    let client = Client::builder()
        .timeout(Duration::from_secs(3600))
        .build()
        .map_err(|err| err.to_string())?;

    tauri::async_runtime::spawn(async move {
        stream_sse_json::<ModelDownloadProgress, _>(
            &app,
            &client,
            &url,
            "model_download_progress",
            "model_download_progress://error",
            |_| {},
            cancel_rx,
        )
        .await;
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
