use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::human_browser;

/// Popout labels that are pre-declared on macOS/Linux and lazily created on Windows.
const POPOUT_LABELS: &[&str] = &[
    "chat-popout",
    "vault-sticky",
    "browser",
    "desktop-toolbar",
    "view-popout",
    "guide",
];

/// Sync WebView2 controller visibility with the OS window.
///
/// On Windows, `WebviewWindow::hide()` only hides the tao/OS window. The
/// `ICoreWebView2Controller.IsVisible` flag can stay true, so the renderer keeps
/// compositing (and Task Manager shows Manager / Network / GPU activity under
/// WebView2) even while the window is invisible. Explicit `SetIsVisible(false)`
/// lets Chromium throttle the page.
#[cfg(windows)]
fn set_webview_visibility(window: &WebviewWindow, visible: bool) {
    let _ = window.with_webview(move |platform| {
        let _ = unsafe { platform.controller().SetIsVisible(visible) };
    });
}

#[cfg(not(windows))]
fn set_webview_visibility(_window: &WebviewWindow, _visible: bool) {}

/// Show/focus a window and resume its WebView2 controller (Windows).
pub fn show_and_resume(window: &WebviewWindow) -> Result<(), String> {
    set_webview_visibility(window, true);
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

/// Hide a popout and suspend its WebView2 controller (Windows).
pub fn hide_and_suspend(window: &WebviewWindow) -> Result<(), String> {
    window.hide().map_err(|err| err.to_string())?;
    set_webview_visibility(window, false);
    Ok(())
}

/// Suspend WebView2 for every non-main popout that already exists.
/// Call after startup when windows were pre-created hidden (macOS/Linux), and
/// after Windows hide paths that go through CloseRequested.
pub fn suspend_hidden_popouts(app: &AppHandle) {
    for label in POPOUT_LABELS {
        if let Some(window) = app.get_webview_window(label) {
            let visible = window.is_visible().unwrap_or(false);
            if !visible {
                set_webview_visibility(&window, false);
            }
        }
    }
}

#[tauri::command]
pub async fn window_show_chat_popout(app: AppHandle) -> Result<(), String> {
    let window = ensure_chat_popout(&app).await?;
    show_and_resume(&window)
}

#[tauri::command]
pub fn window_hide_chat_popout(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("chat-popout") {
        hide_and_suspend(&window)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn window_show_vault_sticky(app: AppHandle) -> Result<(), String> {
    let window = ensure_vault_sticky(&app).await?;
    window
        .set_always_on_top(true)
        .map_err(|err| err.to_string())?;
    show_and_resume(&window)
}

#[tauri::command]
pub fn window_hide_vault_sticky(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("vault-sticky") {
        let _ = window.set_always_on_top(false);
        hide_and_suspend(&window)?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_set_vault_sticky_always_on_top(
    app: AppHandle,
    always_on_top: bool,
) -> Result<(), String> {
    let window = app
        .get_webview_window("vault-sticky")
        .ok_or_else(|| "vault sticky window is not open".to_string())?;
    window
        .set_always_on_top(always_on_top)
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn window_show_browser(app: AppHandle) -> Result<(), String> {
    let window = ensure_browser(&app).await?;
    position_browser_beside_main(&app, &window)?;
    human_browser::prepare_browser_window(&app)?;
    show_and_resume(&window)?;
    human_browser::on_browser_popout_opened(&app)?;
    let _ = app.emit("browser-window-visibility", true);
    Ok(())
}

#[tauri::command]
pub fn window_hide_browser(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("browser") {
        hide_and_suspend(&window)?;
        human_browser::on_browser_popout_closed(&app)?;
        let _ = app.emit("browser-window-visibility", false);
    }
    Ok(())
}

#[tauri::command]
pub async fn window_focus_browser(app: AppHandle) -> Result<(), String> {
    let window = ensure_browser(&app).await?;
    show_and_resume(&window)
}

#[tauri::command]
pub async fn window_show_desktop_toolbar(app: AppHandle) -> Result<(), String> {
    let window = ensure_desktop_toolbar(&app).await?;
    prepare_desktop_toolbar(&window)?;
    show_and_resume(&window)
}

#[tauri::command]
pub fn window_hide_desktop_toolbar(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("desktop-toolbar") {
        hide_and_suspend(&window)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn window_toggle_desktop_toolbar(app: AppHandle) -> Result<bool, String> {
    if let Some(window) = app.get_webview_window("desktop-toolbar") {
        let visible = window.is_visible().map_err(|err| err.to_string())?;
        if visible {
            hide_and_suspend(&window)?;
            return Ok(false);
        }
        prepare_desktop_toolbar(&window)?;
        show_and_resume(&window)?;
        return Ok(true);
    }
    let window = ensure_desktop_toolbar(&app).await?;
    prepare_desktop_toolbar(&window)?;
    show_and_resume(&window)?;
    Ok(true)
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
pub async fn window_show_view_popout(app: AppHandle) -> Result<(), String> {
    let window = ensure_view_popout(&app).await?;
    show_and_resume(&window)
}

#[tauri::command]
pub fn window_hide_view_popout(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("view-popout") {
        hide_and_suspend(&window)?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_show_main(app: AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Err("main window is not configured".to_string());
    };
    window.unminimize().map_err(|err| err.to_string())?;
    set_webview_visibility(&window, true);
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn window_show_guide(app: AppHandle) -> Result<(), String> {
    let window = ensure_guide(&app).await?;
    show_and_resume(&window)
}

#[tauri::command]
pub fn window_hide_guide(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("guide") {
        hide_and_suspend(&window)?;
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
pub async fn browser_window_present(
    app: AppHandle,
    options: BrowserPresentOptions,
) -> Result<(), String> {
    let window = ensure_browser(&app).await?;
    position_browser_beside_main(&app, &window)?;
    human_browser::prepare_browser_window(&app)?;
    show_and_resume(&window)?;
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

async fn ensure_chat_popout(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("chat-popout") {
        return Ok(window);
    }
    create_popout_window(
        app,
        PopoutSpec {
            label: "chat-popout",
            title: "Medousa Chat",
            url: PopoutUrl::App("/popout/chat"),
            width: 440.0,
            height: 760.0,
            min_width: 360.0,
            min_height: 480.0,
            resizable: true,
            decorations: true,
            transparent: false,
            always_on_top: false,
            skip_taskbar: false,
            shadow: true,
        },
    )
    .await
}

async fn ensure_vault_sticky(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("vault-sticky") {
        return Ok(window);
    }
    create_popout_window(
        app,
        PopoutSpec {
            label: "vault-sticky",
            title: "Medousa Note",
            url: PopoutUrl::App("/popout/vault"),
            width: 380.0,
            height: 480.0,
            min_width: 300.0,
            min_height: 360.0,
            resizable: true,
            decorations: true,
            transparent: false,
            always_on_top: false,
            skip_taskbar: false,
            shadow: true,
        },
    )
    .await
}

async fn ensure_browser(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("browser") {
        return Ok(window);
    }
    create_popout_window(
        app,
        PopoutSpec {
            label: "browser",
            title: "Medousa Web",
            url: PopoutUrl::External("about:blank"),
            width: 1280.0,
            height: 900.0,
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
    .await
}

async fn ensure_desktop_toolbar(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("desktop-toolbar") {
        return Ok(window);
    }
    create_popout_window(
        app,
        PopoutSpec {
            label: "desktop-toolbar",
            title: "Medousa Companion",
            url: PopoutUrl::App("/popout/toolbar"),
            width: 112.0,
            height: 170.0,
            min_width: 72.0,
            min_height: 120.0,
            resizable: false,
            decorations: false,
            transparent: true,
            always_on_top: true,
            skip_taskbar: true,
            shadow: false,
        },
    )
    .await
}

async fn ensure_view_popout(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("view-popout") {
        return Ok(window);
    }
    create_popout_window(
        app,
        PopoutSpec {
            label: "view-popout",
            title: "Medousa View",
            url: PopoutUrl::App("/popout/view"),
            width: 960.0,
            height: 700.0,
            min_width: 480.0,
            min_height: 360.0,
            resizable: true,
            decorations: true,
            transparent: false,
            always_on_top: false,
            skip_taskbar: false,
            shadow: true,
        },
    )
    .await
}

async fn ensure_guide(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("guide") {
        return Ok(window);
    }
    create_popout_window(
        app,
        PopoutSpec {
            label: "guide",
            title: "Operator's Guide",
            url: PopoutUrl::App("/popout/guide"),
            width: 860.0,
            height: 920.0,
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
    .await
}

enum PopoutUrl {
    App(&'static str),
    External(&'static str),
}

struct PopoutSpec {
    label: &'static str,
    title: &'static str,
    url: PopoutUrl,
    width: f64,
    height: f64,
    min_width: f64,
    min_height: f64,
    resizable: bool,
    decorations: bool,
    transparent: bool,
    always_on_top: bool,
    skip_taskbar: bool,
    shadow: bool,
}

/// Build a popout on demand. On Windows this must run from an async command /
/// async runtime task — WebView2 creation deadlocks inside sync commands and
/// some event handlers.
async fn create_popout_window(app: &AppHandle, spec: PopoutSpec) -> Result<WebviewWindow, String> {
    // Yield so we are not on a sync WebView2 callback stack.
    tokio::task::yield_now().await;

    if let Some(window) = app.get_webview_window(spec.label) {
        return Ok(window);
    }

    let url = match spec.url {
        PopoutUrl::App(path) => WebviewUrl::App(path.into()),
        PopoutUrl::External(raw) => WebviewUrl::External(
            raw.parse()
                .map_err(|err| format!("invalid url {raw}: {err}"))?,
        ),
    };

    let mut builder = WebviewWindowBuilder::new(app, spec.label, url)
        .title(spec.title)
        .inner_size(spec.width, spec.height)
        .min_inner_size(spec.min_width, spec.min_height)
        .resizable(spec.resizable)
        .decorations(spec.decorations)
        .visible(false)
        .always_on_top(spec.always_on_top)
        .skip_taskbar(spec.skip_taskbar);

    #[cfg(not(target_os = "macos"))]
    {
        builder = builder.shadow(spec.shadow);
    }

    if spec.transparent {
        builder = builder.transparent(true);
    }

    let window = builder.build().map_err(|err| err.to_string())?;
    // Start suspended until the first explicit show — avoids WebView2 compositing
    // a freshly created but still-hidden window.
    set_webview_visibility(&window, false);
    Ok(window)
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
