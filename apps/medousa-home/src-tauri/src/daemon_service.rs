use crate::workshop_runtime::{
    ensure_local_engine, resolve_workshop_url, stop_local_engine, wait_engine_healthy,
};
use crate::workshop_registry::{
    active_workshop, ensure_migrated, WorkshopServer, PERSONAL_WORKSHOP_ID,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStartResult {
    pub started: bool,
    pub already_running: bool,
    pub pid: Option<u32>,
    pub log_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonWaitHealthResult {
    pub ok: bool,
    pub message: String,
    pub attempts: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonWaitHealthRequest {
    #[serde(default = "default_wait_seconds")]
    pub timeout_seconds: u64,
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
}

fn default_wait_seconds() -> u64 {
    30
}

fn default_poll_ms() -> u64 {
    2000
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStartRequest {
    #[serde(default)]
    pub private_brain: bool,
    #[serde(default)]
    pub public_bind: Option<bool>,
}

fn resolve_public_bind(request: Option<&DaemonStartRequest>) -> bool {
    if let Some(explicit) = request.and_then(|value| value.public_bind) {
        return explicit;
    }
    crate::connection_prefs::load_connection_prefs().public_bind
}

async fn ensure_workshop_engine(
    workshop: &WorkshopServer,
    private_brain: bool,
    public_bind: bool,
) -> Result<DaemonStartResult, String> {
    let mut workshop = workshop.clone();
    if workshop.kind == "local" && workshop.id == PERSONAL_WORKSHOP_ID && public_bind {
        workshop.bind = Some(crate::workshop_runtime::public_local_bind().to_string());
        workshop.url = resolve_workshop_url(&workshop);
    }

    let result = ensure_local_engine(&workshop, private_brain).await?;
    Ok(DaemonStartResult {
        started: result.started,
        already_running: result.already_running,
        pid: result.pid,
        log_path: result.log_path,
        message: result.message,
    })
}

pub fn stop_daemon_process() {
    if let Ok(registry) = ensure_migrated() {
        if let Some(workshop) = active_workshop(&registry) {
            if workshop.kind == "local" {
                stop_local_engine(&workshop.id);
                return;
            }
        }
    }
    stop_local_engine(PERSONAL_WORKSHOP_ID);
}

#[tauri::command]
pub async fn daemon_start(request: Option<DaemonStartRequest>) -> Result<DaemonStartResult, String> {
    let request = request.unwrap_or(DaemonStartRequest {
        private_brain: false,
        public_bind: None,
    });
    let private_brain = crate::workshop_runtime::should_load_private_brain(request.private_brain);
    let public_bind = resolve_public_bind(Some(&request));
    let registry = ensure_migrated()?;
    let workshop = active_workshop(&registry)
        .cloned()
        .ok_or_else(|| "No active workshop".to_string())?;
    ensure_workshop_engine(&workshop, private_brain, public_bind).await
}

#[tauri::command]
pub async fn daemon_restart(request: Option<DaemonStartRequest>) -> Result<DaemonStartResult, String> {
    stop_daemon_process();
    tokio::time::sleep(std::time::Duration::from_millis(750)).await;
    daemon_start(request).await
}

#[tauri::command]
pub async fn daemon_wait_healthy(
    request: Option<DaemonWaitHealthRequest>,
) -> Result<DaemonWaitHealthResult, String> {
    let request = request.unwrap_or(DaemonWaitHealthRequest {
        timeout_seconds: default_wait_seconds(),
        poll_ms: default_poll_ms(),
    });
    let registry = ensure_migrated()?;
    let workshop = active_workshop(&registry)
        .cloned()
        .ok_or_else(|| "No active workshop".to_string())?;
    let base_url = resolve_workshop_url(&workshop);
    let (ok, attempts) = wait_engine_healthy(
        &base_url,
        request.timeout_seconds,
        request.poll_ms,
    )
    .await?;
    Ok(DaemonWaitHealthResult {
        ok,
        message: if ok {
            format!("Medousa Engine is ready at {base_url}")
        } else {
            format!(
                "Medousa Engine did not become ready within {}s",
                request.timeout_seconds
            )
        },
        attempts,
    })
}

#[tauri::command]
pub async fn workshop_ensure_engine(
    workshop_id: Option<String>,
    private_brain: Option<bool>,
) -> Result<crate::workshop_runtime::LocalEngineEnsureResult, String> {
    let registry = ensure_migrated()?;
    let workshop = if let Some(id) = workshop_id.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        registry
            .workshops
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| "Workshop not found".to_string())?
    } else {
        active_workshop(&registry)
            .cloned()
            .ok_or_else(|| "No active workshop".to_string())?
    };
    let private_brain =
        crate::workshop_runtime::should_load_private_brain(private_brain.unwrap_or(false));
    ensure_local_engine(&workshop, private_brain).await
}
