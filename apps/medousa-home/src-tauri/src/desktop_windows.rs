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
}

pub fn scope_for(app: &AppHandle, label: &str) -> Option<String> {
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
        return if label.starts_with("desktop-work-") {
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
            return window.hide().map_err(|e| e.to_string());
        }
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
