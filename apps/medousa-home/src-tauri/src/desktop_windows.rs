//! Native resource identities. Titles and navigation never create a second owner.
use crate::window::{create_popout_window, show_and_resume, PopoutSpec, PopoutUrl};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, Manager, State, Webview};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub const SHORTCUT: &str = "CommandOrControl+Shift+Space";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopTarget {
    pub kind: DesktopKind,
    pub workshop_id: String,
    pub resource_id: Option<String>,
    pub path: Option<String>,
    pub title: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DesktopKind {
    Chat,
    Create,
    Code,
}

impl DesktopTarget {
    fn validate(&mut self) -> Result<(), String> {
        self.workshop_id = self.workshop_id.trim().to_string();
        if self.workshop_id.is_empty() || self.workshop_id.len() > 256 {
            return Err("Choose a workshop for this window".into());
        }
        self.resource_id = self.resource_id.take().filter(|s| !s.is_empty());
        if self
            .resource_id
            .as_ref()
            .is_some_and(|s| s.len() > 1024 || s.chars().any(char::is_control))
        {
            return Err("Invalid resource identity".into());
        }
        if let Some(path) = &self.path {
            if self.kind != DesktopKind::Code
                || self.resource_id.is_none()
                || path.len() > 4096
                || path.chars().any(char::is_control)
                || path.starts_with(['/', '\\'])
                || path.contains(':')
                || path.replace('\\', "/").split('/').any(|part| part == "..")
            {
                return Err("Invalid Code window path".into());
            }
        }
        self.title = self
            .title
            .take()
            .map(|s| s.chars().filter(|c| !c.is_control()).take(160).collect());
        Ok(())
    }
    fn label(&self) -> String {
        let identity = serde_json::to_vec(&(&self.kind, &self.workshop_id, &self.resource_id))
            .expect("resource identity");
        format!("desktop-work-{:x}", Sha256::digest(identity))
    }
}

struct ResourceWindow {
    target: DesktopTarget,
    generation: u64,
}

#[derive(Default)]
pub struct DesktopWindows {
    // Serialize the check/build operation, including simultaneous shortcut presses.
    gate: tokio::sync::Mutex<()>,
    targets: std::sync::Mutex<HashMap<String, ResourceWindow>>,
    next_generation: AtomicU64,
    shortcut_error: std::sync::Mutex<Option<String>>,
    spotlight: std::sync::Mutex<Option<ResourceWindow>>,
    spotlight_requests: std::sync::Mutex<
        HashMap<u64, tokio::sync::oneshot::Sender<Result<serde_json::Value, String>>>,
    >,
}

pub fn is_resource_window(label: &str) -> bool {
    crate::active_workshop::is_resource_window(label)
}

fn validate_spotlight_request(
    label: &str,
    scope: Option<&str>,
    requested: &str,
    bytes: usize,
) -> Result<(), String> {
    if label != "desktop-spotlight" || scope != Some(requested) {
        return Err("Spotlight workshop changed. Reopen it and try again.".into());
    }
    if bytes > 128 * 1024 {
        return Err("Spotlight request is too large".into());
    }
    Ok(())
}

pub fn scope_for(app: &AppHandle, label: &str) -> Option<String> {
    if label == "desktop-spotlight" {
        return app
            .state::<DesktopWindows>()
            .spotlight
            .lock()
            .ok()?
            .as_ref()
            .map(|entry| entry.target.workshop_id.clone());
    }
    if !label.starts_with("desktop-work-") {
        return None;
    }
    app.state::<DesktopWindows>()
        .targets
        .lock()
        .ok()?
        .get(label)
        .map(|entry| entry.target.workshop_id.clone())
}

pub fn generation_for(app: &AppHandle, label: &str) -> Option<u64> {
    if label == "desktop-spotlight" {
        return app
            .state::<DesktopWindows>()
            .spotlight
            .lock()
            .ok()?
            .as_ref()
            .map(|entry| entry.generation);
    }
    if !label.starts_with("desktop-work-") {
        return None;
    }
    app.state::<DesktopWindows>()
        .targets
        .lock()
        .ok()?
        .get(label)
        .map(|entry| entry.generation)
}

/// Legacy commands still routed through the selected workshop must fail closed
/// after that selection changes. Scoped editing/streaming continues normally.
pub fn check_command_scope(app: &AppHandle, label: &str, command: &str) -> Result<(), String> {
    let Some(scope) = scope_for(app, label) else {
        return if is_resource_window(label) {
            Err("The work window was closed".into())
        } else {
            Ok(())
        };
    };
    if crate::daemon::window_request::supports_command(command)
        || command.starts_with("desktop_")
        || command.starts_with("window_")
        || command.starts_with("code_lsp_")
        || command.starts_with("terminal_")
        || matches!(
            command,
            "daemon_unary"
                | "daemon_stream_start"
                | "daemon_stream_cancel"
                | "daemon_health"
                | "daemon_url"
                | "forge_request"
                | "code_read"
                | "code_request"
                | "interactive_stream_start"
                | "interactive_stream_stop"
                | "interactive_stream_stop_turn"
                | "workshops_load"
                | "workshops_update_client_state"
                | "workshops_update_branding"
        )
    {
        return Ok(());
    }
    if crate::workshop_registry::ensure_migrated()?.active_workshop_id == scope {
        return Ok(());
    }
    Err("This action uses the main window’s workshop. Switch Medousa back to this window’s workshop to use it. Your open document is still connected to its original workshop.".into())
}

pub fn forget(app: &AppHandle, label: &str) {
    // Remove ownership first; in-flight attach commands check it while holding
    // their registry lock before they are allowed to publish a consumer.
    if let Ok(mut targets) = app.state::<DesktopWindows>().targets.lock() {
        targets.remove(label);
    }
    app.state::<crate::daemon::DaemonState>()
        .cancel_window_streams(label);
    crate::terminal::detach_window(&app.state::<crate::terminal::TerminalRegistry>(), label);
    crate::code_lsp_transport::detach_window(
        &app.state::<crate::code_lsp_transport::CodeLspTransportRegistry>(),
        label,
    );
}

#[tauri::command]
pub fn desktop_window_context(
    window: Webview,
    state: State<'_, DesktopWindows>,
) -> Result<DesktopTarget, String> {
    state
        .targets
        .lock()
        .map_err(|e| e.to_string())?
        .get(window.label())
        .map(|entry| entry.target.clone())
        .ok_or_else(|| "This window has no resource".into())
}

#[tauri::command]
pub fn desktop_shortcut_error(state: State<'_, DesktopWindows>) -> Option<String> {
    state.shortcut_error.lock().ok().and_then(|e| e.clone())
}

#[tauri::command]
pub async fn desktop_open_window(
    app: AppHandle,
    mut target: DesktopTarget,
) -> Result<String, String> {
    target.validate()?;
    crate::active_workshop::resolve_scope_async(target.workshop_id.clone()).await?;
    let state = app.state::<DesktopWindows>();
    let _gate = state.gate.lock().await;
    let label = target.label();
    if let Some(window) = app.get_webview_window(&label) {
        if let Some(entry) = state
            .targets
            .lock()
            .map_err(|e| e.to_string())?
            .get_mut(&label)
        {
            entry.target = target.clone();
        }
        window
            .emit("desktop-resource-navigate", &target)
            .map_err(|e| e.to_string())?;
        show_and_resume(&window)?;
        return Ok(label);
    }
    if state.targets.lock().map_err(|e| e.to_string())?.len() >= 32 {
        return Err("Close a Medousa window before opening another".into());
    }
    state.targets.lock().map_err(|e| e.to_string())?.insert(
        label.clone(),
        ResourceWindow {
            target: target.clone(),
            generation: state.next_generation.fetch_add(1, Ordering::Relaxed) + 1,
        },
    );
    let title = target
        .title
        .as_deref()
        .filter(|t| !t.is_empty())
        .unwrap_or(match target.kind {
            DesktopKind::Chat => "Chat",
            DesktopKind::Create => "Create",
            DesktopKind::Code => "Code",
        });
    let result = create_popout_window(
        &app,
        PopoutSpec {
            label: &label,
            title: &format!("{title} — Medousa"),
            url: PopoutUrl::App("/desktop/work"),
            width: 1180.0,
            height: 820.0,
            min_width: 640.0,
            min_height: 480.0,
            resizable: true,
            decorations: true,
            transparent: false,
            always_on_top: false,
            skip_taskbar: false,
            shadow: true,
        },
    )
    .await;
    match result {
        Ok(window) => {
            show_and_resume(&window)?;
            Ok(label)
        }
        Err(error) => {
            forget(&app, &label);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn desktop_toggle_spotlight(app: AppHandle) -> Result<(), String> {
    let state = app.state::<DesktopWindows>();
    let _gate = state.gate.lock().await;
    if let Some(window) = app.get_webview_window("desktop-spotlight") {
        if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
            window
                .emit("desktop-spotlight-dismiss", ())
                .map_err(|e| e.to_string())?;
            return window.hide().map_err(|e| e.to_string());
        }
    }
    let scope = tauri::async_runtime::spawn_blocking(|| {
        crate::workshop_registry::ensure_migrated().map(|registry| registry.active_workshop_id)
    })
    .await
    .map_err(|e| e.to_string())??;
    let changed = {
        let mut slot = state.spotlight.lock().map_err(|e| e.to_string())?;
        let changed = slot
            .as_ref()
            .is_none_or(|entry| entry.target.workshop_id != scope);
        if changed {
            *slot = Some(ResourceWindow {
                target: DesktopTarget {
                    kind: DesktopKind::Chat,
                    workshop_id: scope,
                    resource_id: None,
                    path: None,
                    title: None,
                },
                generation: state.next_generation.fetch_add(1, Ordering::Relaxed) + 1,
            });
        }
        changed
    };
    if changed {
        app.state::<crate::daemon::DaemonState>()
            .cancel_window_streams("desktop-spotlight");
    }
    if let Some(window) = app.get_webview_window("desktop-spotlight") {
        window
            .emit("desktop-spotlight-open", ())
            .map_err(|e| e.to_string())?;
        return show_and_resume(&window);
    }
    let window = create_popout_window(
        &app,
        PopoutSpec {
            label: "desktop-spotlight",
            title: "Medousa Spotlight",
            url: PopoutUrl::App("/desktop/spotlight"),
            width: 880.0,
            height: 600.0,
            min_width: 640.0,
            min_height: 480.0,
            resizable: false,
            decorations: false,
            transparent: true,
            always_on_top: true,
            skip_taskbar: true,
            shadow: true,
        },
    )
    .await?;
    crate::window::prepare_floating_window(&window, true)?;
    #[cfg(target_os = "macos")]
    window
        .set_visible_on_all_workspaces(true)
        .map_err(|e| e.to_string())?;
    window.center().map_err(|e| e.to_string())?;
    show_and_resume(&window)
}

/// Pin legacy actions and stream consumers to the workshop shown by this opening.
#[tauri::command]
pub fn desktop_spotlight_context(app: AppHandle, window: Webview) -> Result<String, String> {
    if window.label() != "desktop-spotlight" {
        return Err("This command belongs to Desktop Spotlight".into());
    }
    scope_for(&app, window.label())
        .ok_or("Open Desktop Spotlight again to choose a workshop".into())
}

/// Restore a failed handoff without changing the launcher's retained workshop.
#[tauri::command]
pub fn desktop_spotlight_restore(
    app: AppHandle,
    window: Webview,
    workshop_id: String,
) -> Result<(), String> {
    validate_spotlight_request(
        window.label(),
        scope_for(&app, window.label()).as_deref(),
        &workshop_id,
        0,
    )?;
    show_and_resume(
        &app.get_webview_window("desktop-spotlight")
            .ok_or("Desktop Spotlight closed")?,
    )
}

/// Route known UI commands to their actual shell owner. No code or functions cross IPC.
#[tauri::command]
pub async fn desktop_spotlight_request(
    app: AppHandle,
    window: Webview,
    workshop_id: String,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    validate_spotlight_request(
        window.label(),
        scope_for(&app, window.label()).as_deref(),
        &workshop_id,
        serde_json::to_vec(&request)
            .map_err(|e| e.to_string())?
            .len(),
    )?;
    let generation = generation_for(&app, window.label());
    let state = app.state::<DesktopWindows>();
    let id = state.next_generation.fetch_add(1, Ordering::Relaxed) + 1;
    let (tx, rx) = tokio::sync::oneshot::channel();
    {
        let mut requests = state.spotlight_requests.lock().map_err(|e| e.to_string())?;
        if requests.len() >= 16 {
            return Err("Wait for the current Spotlight action to finish".into());
        }
        requests.insert(id, tx);
    }
    let send = app.emit_to(
        "main",
        "desktop-spotlight-request",
        serde_json::json!({"id":id,"workshopId":workshop_id,"request":request}),
    );
    if let Err(error) = send {
        state
            .spotlight_requests
            .lock()
            .map_err(|e| e.to_string())?
            .remove(&id);
        return Err(error.to_string());
    }
    let wait = if request.get("kind").and_then(serde_json::Value::as_str) == Some("collect") {
        10
    } else {
        120
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(wait), rx).await;
    state
        .spotlight_requests
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&id);
    if generation != generation_for(&app, window.label()) {
        return Err("Spotlight changed workshops while this request was processing. Check Medousa before retrying an action.".into());
    }
    result.map_err(|_| "Medousa did not acknowledge this request. Check the main window before retrying an action.".to_string())?
        .map_err(|_| "The Spotlight command owner closed".to_string())?
}

#[tauri::command]
pub fn desktop_spotlight_reply(
    app: AppHandle,
    window: Webview,
    id: u64,
    result: Option<serde_json::Value>,
    error: Option<String>,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Only the main shell can acknowledge its commands".into());
    }
    if let Some(tx) = app
        .state::<DesktopWindows>()
        .spotlight_requests
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&id)
    {
        let _ = tx.send(match error {
            Some(error) => Err(error),
            None => Ok(result.unwrap_or(serde_json::Value::Null)),
        });
    }
    Ok(())
}

pub fn install_shortcut(app: &AppHandle) -> tauri::Result<()> {
    let installed = app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _, event| {
                if event.state() == ShortcutState::Pressed {
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(error) = desktop_toggle_spotlight(app).await {
                            eprintln!("Desktop Spotlight: {error}");
                        }
                    });
                }
            })
            .build(),
    );
    if let Err(error) = installed {
        let message = format!("Desktop shortcut is unavailable: {error}. Open Spotlight from the Medousa menu bar menu.");
        eprintln!("{message}");
        *app.state::<DesktopWindows>()
            .shortcut_error
            .lock()
            .expect("shortcut status") = Some(message);
        return Ok(());
    }
    if let Err(error) = app.global_shortcut().register(SHORTCUT) {
        let message = format!(
            "Couldn’t register {SHORTCUT}: {error}. Open Spotlight from the Medousa menu bar menu."
        );
        eprintln!("{message}");
        *app.state::<DesktopWindows>()
            .shortcut_error
            .lock()
            .expect("shortcut status") = Some(message);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn spotlight_requests_require_native_launcher_identity_and_pinned_scope() {
        use super::validate_spotlight_request;
        assert!(validate_spotlight_request("desktop-spotlight", Some("a"), "a", 1024).is_ok());
        assert!(validate_spotlight_request("main", Some("a"), "a", 1024).is_err());
        assert!(validate_spotlight_request("desktop-work-test", Some("a"), "a", 1024).is_err());
        assert!(validate_spotlight_request("desktop-spotlight", Some("b"), "a", 1024).is_err());
        assert!(validate_spotlight_request("desktop-spotlight", None, "a", 1024).is_err());
        assert!(
            validate_spotlight_request("desktop-spotlight", Some("a"), "a", 128 * 1024 + 1)
                .is_err()
        );
    }

    #[test]
    fn launcher_and_work_window_consumers_survive_main_workshop_switches() {
        use super::is_resource_window;
        assert!(is_resource_window("desktop-spotlight"));
        assert!(is_resource_window("desktop-work-test"));
        assert!(!is_resource_window("main"));
        assert!(!is_resource_window("desktop-spotlight-forged"));
    }
    use super::*;
    fn target() -> DesktopTarget {
        DesktopTarget {
            kind: DesktopKind::Code,
            workshop_id: "personal".into(),
            resource_id: Some("project".into()),
            path: None,
            title: None,
        }
    }
    #[test]
    fn navigation_and_titles_reuse_identity_but_scopes_and_kinds_do_not() {
        let a = target();
        let mut b = a.clone();
        b.path = Some("src/main.rs".into());
        b.title = Some("Renamed".into());
        assert_eq!(a.label(), b.label());
        b.workshop_id = "remote".into();
        assert_ne!(a.label(), b.label());
        b = a.clone();
        b.kind = DesktopKind::Create;
        assert_ne!(a.label(), b.label());
        b = a.clone();
        b.resource_id = None;
        assert_ne!(a.label(), b.label());
    }
    #[test]
    fn validates_bounded_resource_requests() {
        let mut t = target();
        t.workshop_id.clear();
        assert!(t.validate().is_err());
        t = target();
        t.kind = DesktopKind::Chat;
        t.path = Some("file".into());
        assert!(t.validate().is_err());
        t = target();
        t.resource_id = Some("bad\n".into());
        assert!(t.validate().is_err());
    }
}
