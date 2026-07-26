use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::human_browser;

#[tauri::command]
pub fn window_show_chat_popout(app: AppHandle) -> Result<(), String> {
    let window = chat_popout_window(&app)?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn window_hide_chat_popout(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("chat-popout") {
        window.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_show_vault_sticky(app: AppHandle) -> Result<(), String> {
    let window = vault_sticky_window(&app)?;
    window
        .set_always_on_top(true)
        .map_err(|err| err.to_string())?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn window_hide_vault_sticky(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("vault-sticky") {
        let _ = window.set_always_on_top(false);
        window.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_set_vault_sticky_always_on_top(
    app: AppHandle,
    always_on_top: bool,
) -> Result<(), String> {
    let window = vault_sticky_window(&app)?;
    window
        .set_always_on_top(always_on_top)
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn window_show_browser(app: AppHandle) -> Result<(), String> {
    let window = browser_webview_window(&app)?;
    position_browser_beside_main(&app, &window)?;
    human_browser::prepare_browser_window(&app)?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    human_browser::on_browser_popout_opened(&app)?;
    let _ = app.emit("browser-window-visibility", true);
    Ok(())
}

#[tauri::command]
pub fn window_hide_browser(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("browser") {
        window.hide().map_err(|err| err.to_string())?;
        human_browser::on_browser_popout_closed(&app)?;
        let _ = app.emit("browser-window-visibility", false);
    }
    Ok(())
}

#[tauri::command]
pub fn window_focus_browser(app: AppHandle) -> Result<(), String> {
    let window = browser_window(&app)?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn window_show_desktop_toolbar(app: AppHandle) -> Result<(), String> {
    let window = desktop_toolbar_window(&app)?;
    prepare_desktop_toolbar(&window)?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn window_hide_desktop_toolbar(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("desktop-toolbar") {
        window.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_toggle_desktop_toolbar(app: AppHandle) -> Result<bool, String> {
    let window = desktop_toolbar_window(&app)?;
    let visible = window.is_visible().map_err(|err| err.to_string())?;
    if visible {
        window.hide().map_err(|err| err.to_string())?;
        Ok(false)
    } else {
        prepare_desktop_toolbar(&window)?;
        window.show().map_err(|err| err.to_string())?;
        window.set_focus().map_err(|err| err.to_string())?;
        Ok(true)
    }
}

fn prepare_desktop_toolbar(window: &WebviewWindow) -> Result<(), String> {
    window
        .set_always_on_top(true)
        .map_err(|err| err.to_string())?;
    clear_desktop_toolbar_native_background(window);
    Ok(())
}

/// Kill the default opaque WKWebView / NSWindow fill so CSS transparency can show through.
fn clear_desktop_toolbar_native_background(window: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSColor, NSWindow};

        if let Ok(ptr) = window.ns_window() {
            let ns_window = unsafe { &*(ptr as *const NSWindow) };
            ns_window.setOpaque(false);
            ns_window.setBackgroundColor(Some(&NSColor::clearColor()));
        }
    }
    let _ = window;
}

#[tauri::command]
pub fn window_show_view_popout(app: AppHandle) -> Result<(), String> {
    let window = view_popout_window(&app)?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn window_hide_view_popout(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("view-popout") {
        window.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_show_main(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Err("main window is not configured".to_string());
    };
    window.unminimize().map_err(|err| err.to_string())?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn window_show_guide(app: AppHandle) -> Result<(), String> {
    let window = guide_window(&app)?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn window_hide_guide(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("guide") {
        window.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPresentOptions {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub work_card_id: Option<String>,
    #[serde(default)]
    pub opened_by: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub open_workshop: bool,
}

#[tauri::command]
pub fn browser_window_present(app: AppHandle, options: BrowserPresentOptions) -> Result<(), String> {
    let window = browser_webview_window(&app)?;
    position_browser_beside_main(&app, &window)?;
    human_browser::prepare_browser_window(&app)?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    human_browser::on_browser_popout_opened(&app)?;
    let _ = app.emit("browser-window-visibility", true);
    if let Some(url) = options.url.as_deref().filter(|u| !u.trim().is_empty()) {
        let app_clone = app.clone();
        let url = url.to_string();
        tauri::async_runtime::spawn(async move {
            let _ = human_browser::human_browser_popout_navigate(app_clone, url).await;
        });
    }
    let _ = app.emit("browser-present", options);
    Ok(())
}

fn chat_popout_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("chat-popout")
        .ok_or_else(|| "chat popout window is not configured".to_string())
}

fn vault_sticky_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("vault-sticky")
        .ok_or_else(|| "vault sticky window is not configured".to_string())
}

fn browser_webview_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("browser")
        .ok_or_else(|| "browser window is not configured".to_string())
}

fn browser_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    browser_webview_window(app)
}

fn desktop_toolbar_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("desktop-toolbar")
        .ok_or_else(|| "desktop toolbar window is not configured".to_string())
}

fn view_popout_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("view-popout")
        .ok_or_else(|| "view popout window is not configured".to_string())
}

fn guide_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("guide")
        .ok_or_else(|| "guide window is not configured".to_string())
}

/// Place the browser window to the right of main on first show (when still at default position).
fn position_browser_beside_main(app: &AppHandle, browser: &WebviewWindow) -> Result<(), String> {
    let Some(main) = app.get_webview_window("main") else {
        return Ok(());
    };

    let main_pos = main.outer_position().map_err(|err| err.to_string())?;
    let main_size = main.outer_size().map_err(|err| err.to_string())?;
    let browser_pos = browser.outer_position().map_err(|err| err.to_string())?;

    // Skip if the user has already moved the browser window away from origin.
    if browser_pos.x > 16 || browser_pos.y > 16 {
        return Ok(());
    }

    let gap: i32 = 12;
    let x = main_pos
        .x
        .saturating_add(main_size.width as i32)
        .saturating_add(gap);
    let y = main_pos.y;
    browser
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|err| err.to_string())?;
    Ok(())
}
