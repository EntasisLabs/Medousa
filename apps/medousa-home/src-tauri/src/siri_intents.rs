use serde::{Deserialize, Serialize};

#[cfg(target_os = "ios")]
use std::sync::OnceLock;
#[cfg(target_os = "ios")]
use tauri::{AppHandle, Manager};

#[cfg(target_os = "ios")]
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
#[cfg(target_os = "ios")]
static SIRI_COMPLETIONS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(target_os = "ios")]
pub fn init_app_handle(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

#[cfg(target_os = "ios")]
fn pending_completion_path() -> std::path::PathBuf {
    crate::paths::medousa_data_dir().join("siri-pending-completions.json")
}

#[cfg(target_os = "ios")]
fn pending_completions() -> Vec<String> {
    let _guard = SIRI_COMPLETIONS_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    std::fs::read_to_string(pending_completion_path())
        .ok()
        .and_then(|encoded| serde_json::from_str(&encoded).ok())
        .unwrap_or_default()
}

#[cfg(target_os = "ios")]
fn update_pending_completion(turn_id: &str, add: bool) {
    let _guard = SIRI_COMPLETIONS_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let path = pending_completion_path();
    let mut ids: Vec<String> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|encoded| serde_json::from_str(&encoded).ok())
        .unwrap_or_default();
    ids.retain(|id| id != turn_id);
    if add {
        ids.push(turn_id.to_string());
    }
    if let Ok(encoded) = serde_json::to_vec(&ids) {
        let _ = std::fs::write(path, encoded);
    }
}

#[cfg(target_os = "ios")]
async fn reconcile_completion(app: AppHandle, turn_id: String) {
    use medousa_types::TurnStreamEventV2;
    let embedded = app.state::<crate::embedded_daemon::EmbeddedDaemonState>();
    let Ok(Some(client)) = embedded.client_if_active().await else {
        return;
    };
    let Ok(mut stream) = client.subscribe_turn(&turn_id, 0).await else {
        return;
    };
    while let Ok(Some(envelope)) = stream.recv().await {
        let text = match envelope.event {
            TurnStreamEventV2::Final { text, .. }
            | TurnStreamEventV2::WorkerSynthesis { text, .. } => Some(text),
            TurnStreamEventV2::NeedsInput { .. } | TurnStreamEventV2::Checkpoint { .. } => {
                Some("Medousa needs you to open the app to finish your request.".to_string())
            }
            TurnStreamEventV2::Error { operator_message, .. } => Some(operator_message),
            _ => None,
        };
        if let Some(text) = text {
            ios::notify_completion(&text);
            update_pending_completion(&turn_id, false);
            return;
        }
    }
}

#[cfg(target_os = "ios")]
pub fn reconcile_pending_completions(app: AppHandle) {
    for turn_id in pending_completions() {
        let app = app.clone();
        tauri::async_runtime::spawn(reconcile_completion(app, turn_id));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSiriAsk {
    pub request_id: String,
    pub prompt: String,
    pub workshop_id: String,
    pub created_at: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiriExecutionContextInput {
    pub session_id: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_response_depth")]
    pub response_depth_mode: String,
    #[serde(default)]
    pub reasoning_effort: String,
    #[serde(default)]
    pub identity_user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SiriPreferencesInput {
    pub speech_mode: String,
    pub max_spoken_characters: usize,
    #[serde(default)]
    pub default_workshop_id: Option<String>,
    #[serde(default)]
    pub default_session_id: Option<String>,
    #[serde(default)]
    pub fast_response_model: Option<String>,
}

fn default_response_depth() -> String {
    "standard".to_string()
}

#[cfg(target_os = "ios")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SiriExecutionContext {
    version: u32,
    workshop_id: String,
    base_url: String,
    session_id: String,
    provider: String,
    model: String,
    response_depth_mode: String,
    reasoning_effort: String,
    identity_user_id: Option<String>,
    updated_at: f64,
}

#[cfg(target_os = "ios")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SiriPersonalTurnRequest {
    prompt: String,
    session_id: String,
    provider: String,
    model: String,
    response_depth_mode: String,
    reasoning_effort: String,
    identity_user_id: Option<String>,
    result_wait_seconds: Option<f64>,
}

#[cfg(target_os = "ios")]
async fn execute_personal_turn(
    request: SiriPersonalTurnRequest,
) -> Result<serde_json::Value, String> {
    use medousa_types::{TurnStreamEventV2, TurnSurfaceContext};

    let app = tokio::time::timeout(std::time::Duration::from_secs(8), async {
        loop {
            if let Some(app) = APP_HANDLE.get() {
                return app;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_| "Medousa background runtime did not become ready".to_string())?;
    app.state::<crate::embedded_daemon::EmbeddedDaemonState>()
        .resume_for_background_execution()
        .await?;
    let embedded = app.state::<crate::embedded_daemon::EmbeddedDaemonState>();
    let client = embedded
        .client_if_active_for_route(
            (!request.provider.trim().is_empty()).then_some(request.provider.as_str()),
            (!request.model.trim().is_empty()).then_some(request.model.as_str()),
        )
        .await?
        .ok_or_else(|| "Personal is no longer the selected workshop".to_string())?;
    let active_turn = client
        .active_turn(&request.session_id)
        .await
        .map_err(|error| error.to_string())?
        .turn;
    if let Some(active) = active_turn {
        if pending_completions().contains(&active.turn_id) {
            client
                .cancel_active_turn(&request.session_id)
                .await
                .map_err(|error| error.to_string())?;
            update_pending_completion(&active.turn_id, false);
        }
    }
    let accepted = client
        .start_turn_with_options(
            &request.session_id,
            request.prompt,
            request.identity_user_id,
            TurnSurfaceContext {
                channel_surface: Some("home-ios-siri".to_string()),
                channel_id: Some(request.session_id.clone()),
                user_id: None,
                supports_ui_artifacts: false,
                supports_liquid_markdown: false,
                supports_browser_host: false,
                browser_driver_id: None,
                selected_worlds: Vec::new(),
            },
            None,
            None,
            request.response_depth_mode,
            request.reasoning_effort,
            Vec::new(),
            Vec::new(),
            None,
        )
        .await
        .map_err(|error| error.to_string())?;
    let result_wait_seconds = request
        .result_wait_seconds
        .unwrap_or(12.0)
        .clamp(12.0, 120.0);
    let watchdog_seconds = result_wait_seconds + 16.0;
    let watchdog_client = client.clone();
    let watchdog_session_id = request.session_id.clone();
    let watchdog_turn_id = accepted.turn_id.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs_f64(watchdog_seconds)).await;
        let active = watchdog_client.active_turn(&watchdog_session_id).await.ok();
        if active
            .and_then(|response| response.turn)
            .is_some_and(|turn| turn.turn_id == watchdog_turn_id)
        {
            let _ = watchdog_client.cancel_active_turn(&watchdog_session_id).await;
            update_pending_completion(&watchdog_turn_id, false);
            ios::notify_completion(
                "That request needs Medousa open to continue using its tools.",
            );
        }
    });
    let mut stream = client
        .subscribe_turn(&accepted.turn_id, 0)
        .await
        .map_err(|error| error.to_string())?;
    let result = tokio::time::timeout(
        std::time::Duration::from_secs_f64(result_wait_seconds),
        async {
            while let Some(envelope) = stream.recv().await.map_err(|error| error.to_string())? {
                match envelope.event {
                    TurnStreamEventV2::Final { text, .. }
                    | TurnStreamEventV2::WorkerSynthesis { text, .. } => {
                        return Ok(("answer", text));
                    }
                    TurnStreamEventV2::NeedsInput { text, .. }
                    | TurnStreamEventV2::Checkpoint { text, .. } => {
                        return Ok(("needs_input", text));
                    }
                    TurnStreamEventV2::Error {
                        operator_message, ..
                    } => return Err(operator_message),
                    _ => {}
                }
            }
            Err("Medousa's response stream ended early".to_string())
        },
    )
    .await;
    match result {
        Ok(Ok((status, text))) => {
            update_pending_completion(&accepted.turn_id, false);
            Ok(serde_json::json!({ "status": status, "text": text }))
        }
        Ok(Err(error)) => {
            update_pending_completion(&accepted.turn_id, false);
            Err(error)
        }
        Err(_) => {
            let turn_id = accepted.turn_id.clone();
            update_pending_completion(&turn_id, true);
            tauri::async_runtime::spawn(async move {
                while let Ok(Some(envelope)) = stream.recv().await {
                    match envelope.event {
                        TurnStreamEventV2::Final { text, .. }
                        | TurnStreamEventV2::WorkerSynthesis { text, .. } => {
                            ios::notify_completion(&text);
                            update_pending_completion(&turn_id, false);
                            break;
                        }
                        TurnStreamEventV2::NeedsInput { .. }
                        | TurnStreamEventV2::Checkpoint { .. } => {
                            ios::notify_completion(
                                "Medousa needs you to open the app to finish your request.",
                            );
                            update_pending_completion(&turn_id, false);
                            break;
                        }
                        TurnStreamEventV2::Error { operator_message, .. } => {
                            ios::notify_completion(&operator_message);
                            update_pending_completion(&turn_id, false);
                            break;
                        }
                        _ => {}
                    }
                }
            });
            Ok(serde_json::json!({ "status": "continuing" }))
        }
    }
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn medousa_siri_execute_personal(
    json: *const std::os::raw::c_char,
) -> *mut std::os::raw::c_char {
    if json.is_null() {
        return std::ptr::null_mut();
    }
    let result = std::panic::catch_unwind(|| {
        let encoded = unsafe { std::ffi::CStr::from_ptr(json) }
            .to_str()
            .map_err(|error| error.to_string())?;
        let request: SiriPersonalTurnRequest =
            serde_json::from_str(encoded).map_err(|error| error.to_string())?;
        tauri::async_runtime::block_on(execute_personal_turn(request))
    });
    let payload = match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => serde_json::json!({ "status": "error", "error": error }),
        Err(_) => {
            serde_json::json!({ "status": "error", "error": "Medousa background runtime failed" })
        }
    };
    std::ffi::CString::new(payload.to_string())
        .map(std::ffi::CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn medousa_siri_free_rust_string(value: *mut std::os::raw::c_char) {
    if !value.is_null() {
        drop(unsafe { std::ffi::CString::from_raw(value) });
    }
}

#[cfg(target_os = "ios")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SiriWorkshopSummary {
    id: String,
    name: String,
    is_active: bool,
}

#[tauri::command]
pub fn siri_sync_workshop_snapshot() -> Result<(), String> {
    #[cfg(target_os = "ios")]
    {
        let registry = crate::workshop_registry::ensure_migrated()?;
        let active_workshop_id = registry.active_workshop_id.clone();
        let summaries = registry
            .workshops
            .into_iter()
            .filter(|workshop| crate::workshop_registry::is_portal_kind(&workshop.kind))
            .map(|workshop| SiriWorkshopSummary {
                is_active: workshop.id == active_workshop_id,
                id: workshop.id,
                name: workshop.label,
            })
            .collect::<Vec<_>>();
        return ios::store_workshops(&summaries);
    }

    #[cfg(not(target_os = "ios"))]
    Ok(())
}

#[tauri::command]
pub fn siri_sync_execution_context(context: SiriExecutionContextInput) -> Result<(), String> {
    let session_id = context.session_id.trim();
    if session_id.is_empty() || session_id.len() > 256 {
        return Err("Invalid Siri session context".into());
    }

    #[cfg(target_os = "ios")]
    {
        let registry = crate::workshop_registry::ensure_migrated()?;
        let workshop = crate::workshop_registry::active_workshop(&registry)
            .ok_or_else(|| "No active workshop in registry".to_string())?;
        let (base_url, bearer) = match crate::active_workshop::resolve()? {
            crate::active_workshop::ActiveWorkshopTarget::EmbeddedPersonal => {
                (crate::daemon::types::DEFAULT_DAEMON_URL.to_string(), None)
            }
            crate::active_workshop::ActiveWorkshopTarget::Transport { base_url, .. } => {
                let transport = crate::active_workshop::transport_config()?;
                (base_url, transport.session_token)
            }
        };
        let snapshot = SiriExecutionContext {
            version: 1,
            workshop_id: workshop.id.clone(),
            base_url,
            session_id: session_id.to_string(),
            provider: context.provider.trim().to_string(),
            model: context.model.trim().to_string(),
            response_depth_mode: context.response_depth_mode.trim().to_string(),
            reasoning_effort: context.reasoning_effort.trim().to_string(),
            identity_user_id: context
                .identity_user_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            updated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_secs_f64(),
        };
        return ios::store_execution_context(&snapshot, bearer.as_deref());
    }

    #[cfg(not(target_os = "ios"))]
    Ok(())
}

#[tauri::command]
pub fn siri_sync_preferences(preferences: SiriPreferencesInput) -> Result<(), String> {
    if !matches!(preferences.speech_mode.as_str(), "auto" | "always" | "never") {
        return Err("Invalid Siri speech mode".into());
    }
    if !(80..=1_000).contains(&preferences.max_spoken_characters) {
        return Err("Siri spoken length must be between 80 and 1000 characters".into());
    }
    for value in [
        preferences.default_workshop_id.as_deref(),
        preferences.default_session_id.as_deref(),
        preferences.fast_response_model.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if value.len() > 256 || value.chars().any(char::is_control) {
            return Err("Invalid Siri preference value".into());
        }
    }

    #[cfg(target_os = "ios")]
    return ios::store_preferences(&preferences);

    #[cfg(not(target_os = "ios"))]
    Ok(())
}

#[tauri::command]
pub fn siri_consume_pending_ask(request_id: String) -> Result<PendingSiriAsk, String> {
    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.len() > 64 {
        return Err("Invalid Siri request receipt".into());
    }

    #[cfg(target_os = "ios")]
    {
        return ios::consume(request_id);
    }

    #[cfg(not(target_os = "ios"))]
    Err("Siri requests are only available on iOS".into())
}

#[tauri::command]
pub fn siri_recent_pending_ask_id() -> Result<Option<String>, String> {
    #[cfg(target_os = "ios")]
    {
        return ios::recent_pending_id();
    }

    #[cfg(not(target_os = "ios"))]
    Ok(None)
}

#[tauri::command]
pub fn siri_publish_ask_result(request_id: String, text: String) -> Result<(), String> {
    let request_id = request_id.trim();
    let text = text.trim();
    if request_id.is_empty() || request_id.len() > 64 || text.is_empty() || text.len() > 2_000 {
        return Err("Invalid Siri result payload".into());
    }

    #[cfg(target_os = "ios")]
    {
        return ios::publish_result(request_id, text);
    }

    #[cfg(not(target_os = "ios"))]
    Ok(())
}

#[cfg(target_os = "ios")]
mod ios {
    use super::PendingSiriAsk;
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    #[cfg(live_activity_native)]
    unsafe extern "C" {
        fn medousa_siri_consume_pending_ask(request_id: *const c_char) -> *mut c_char;
        fn medousa_siri_recent_pending_ask_id() -> *mut c_char;
        fn medousa_siri_publish_ask_result(json: *const c_char) -> bool;
        fn medousa_siri_store_execution_context(json: *const c_char, bearer: *const c_char)
        -> bool;
        fn medousa_siri_store_preferences(json: *const c_char) -> bool;
        fn medousa_siri_notify_completion(body: *const c_char) -> bool;
        fn medousa_siri_store_workshops(json: *const c_char) -> bool;
        fn medousa_live_activity_free_string(ptr: *mut c_char);
    }

    pub fn consume(request_id: &str) -> Result<PendingSiriAsk, String> {
        #[cfg(live_activity_native)]
        {
            let request_id =
                CString::new(request_id).map_err(|_| "Invalid Siri request receipt".to_string())?;
            let raw = unsafe { medousa_siri_consume_pending_ask(request_id.as_ptr()) };
            if raw.is_null() {
                return Err("Siri request expired or was already consumed".into());
            }
            let encoded = unsafe {
                let encoded = CStr::from_ptr(raw).to_string_lossy().into_owned();
                medousa_live_activity_free_string(raw);
                encoded
            };
            let pending: PendingSiriAsk = serde_json::from_str(&encoded)
                .map_err(|error| format!("Invalid Siri request payload: {error}"))?;
            let prompt = pending.prompt.trim();
            if pending.request_id != request_id.to_string_lossy()
                || prompt.is_empty()
                || prompt.len() > 4_000
            {
                return Err("Invalid Siri request payload".into());
            }
            return Ok(PendingSiriAsk {
                prompt: prompt.to_string(),
                ..pending
            });
        }

        #[cfg(not(live_activity_native))]
        Err("Siri native bridge is unavailable".into())
    }

    pub fn recent_pending_id() -> Result<Option<String>, String> {
        #[cfg(live_activity_native)]
        {
            let raw = unsafe { medousa_siri_recent_pending_ask_id() };
            if raw.is_null() {
                return Ok(None);
            }
            let request_id = unsafe {
                let request_id = CStr::from_ptr(raw).to_string_lossy().into_owned();
                medousa_live_activity_free_string(raw);
                request_id
            };
            if request_id.is_empty() || request_id.len() > 64 {
                return Err("Invalid Siri request receipt".into());
            }
            return Ok(Some(request_id));
        }

        #[cfg(not(live_activity_native))]
        Ok(None)
    }

    pub fn publish_result(request_id: &str, text: &str) -> Result<(), String> {
        #[cfg(live_activity_native)]
        {
            let encoded = serde_json::json!({
                "requestId": request_id,
                "text": text,
                "createdAt": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|error| error.to_string())?
                    .as_secs_f64(),
            })
            .to_string();
            let encoded = CString::new(encoded)
                .map_err(|_| "Siri result contained a null byte".to_string())?;
            if unsafe { medousa_siri_publish_ask_result(encoded.as_ptr()) } {
                return Ok(());
            }
            return Err("Could not publish the Siri result".into());
        }

        #[cfg(not(live_activity_native))]
        Err("Siri native bridge is unavailable".into())
    }

    pub fn store_execution_context(
        context: &super::SiriExecutionContext,
        bearer: Option<&str>,
    ) -> Result<(), String> {
        #[cfg(live_activity_native)]
        {
            let encoded = serde_json::to_string(context).map_err(|error| error.to_string())?;
            let encoded = CString::new(encoded)
                .map_err(|_| "Siri execution context contained a null byte".to_string())?;
            let bearer = CString::new(bearer.unwrap_or_default())
                .map_err(|_| "Siri bearer contained a null byte".to_string())?;
            if unsafe { medousa_siri_store_execution_context(encoded.as_ptr(), bearer.as_ptr()) } {
                return Ok(());
            }
            return Err("Could not store the Siri execution context".into());
        }

        #[cfg(not(live_activity_native))]
        Err("Siri native bridge is unavailable".into())
    }

    pub fn store_preferences(preferences: &super::SiriPreferencesInput) -> Result<(), String> {
        #[cfg(live_activity_native)]
        {
            let encoded = serde_json::to_string(preferences).map_err(|error| error.to_string())?;
            let encoded = CString::new(encoded)
                .map_err(|_| "Siri preferences contained a null byte".to_string())?;
            if unsafe { medousa_siri_store_preferences(encoded.as_ptr()) } {
                return Ok(());
            }
            return Err("Could not store Siri preferences".into());
        }

        #[cfg(not(live_activity_native))]
        Err("Siri native bridge is unavailable".into())
    }

    pub fn notify_completion(body: &str) {
        #[cfg(live_activity_native)]
        if let Ok(body) = CString::new(body.chars().take(500).collect::<String>()) {
            let _ = unsafe { medousa_siri_notify_completion(body.as_ptr()) };
        }
    }

    pub fn store_workshops(summaries: &[super::SiriWorkshopSummary]) -> Result<(), String> {
        #[cfg(live_activity_native)]
        {
            let encoded = serde_json::to_string(summaries).map_err(|error| error.to_string())?;
            let encoded = CString::new(encoded)
                .map_err(|_| "Workshop snapshot contained a null byte".to_string())?;
            if unsafe { medousa_siri_store_workshops(encoded.as_ptr()) } {
                return Ok(());
            }
            return Err("Could not store the Siri workshop snapshot".into());
        }

        #[cfg(not(live_activity_native))]
        Err("Siri native bridge is unavailable".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_receipt_before_platform_dispatch() {
        assert!(siri_consume_pending_ask("".into()).is_err());
        assert!(siri_consume_pending_ask("x".repeat(65)).is_err());
    }

    #[test]
    fn rejects_invalid_result_before_platform_dispatch() {
        assert!(siri_publish_ask_result("".into(), "answer".into()).is_err());
        assert!(siri_publish_ask_result("receipt".into(), "".into()).is_err());
        assert!(siri_publish_ask_result("receipt".into(), "x".repeat(2_001)).is_err());
    }

    #[test]
    fn rejects_invalid_execution_context_before_platform_dispatch() {
        let context = SiriExecutionContextInput {
            session_id: "".into(),
            provider: "openai".into(),
            model: "model".into(),
            response_depth_mode: "standard".into(),
            reasoning_effort: "default".into(),
            identity_user_id: None,
        };
        assert!(siri_sync_execution_context(context).is_err());
    }

    #[test]
    fn rejects_invalid_siri_preferences_before_platform_dispatch() {
        let preferences = SiriPreferencesInput {
            speech_mode: "sometimes".into(),
            max_spoken_characters: 40,
            default_workshop_id: None,
            default_session_id: None,
            fast_response_model: None,
        };
        assert!(siri_sync_preferences(preferences).is_err());
    }
}
