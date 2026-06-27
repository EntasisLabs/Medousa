//! Human-first browser: fixed Rust layout, no DOM-measured overlays.
//!
//! - `browser-content` child webview: external URLs (primary rendering surface)
//! - `browser-chrome` child webview: Svelte tab/URL chrome strip
//! - Main window webview stays `about:blank` behind children (required by Tauri)

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, WebviewUrl};

const BROWSER_WINDOW_LABEL: &str = "browser";
const BROWSER_CONTENT_LABEL: &str = "browser-content";
const BROWSER_CHROME_LABEL: &str = "browser-chrome";
const CHROME_HEIGHT_LOGICAL: f64 = 132.0;

fn main_webview(app: &AppHandle) -> Option<tauri::Webview> {
    app.get_webview(BROWSER_WINDOW_LABEL)
}

static SHELL_READY: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanBrowserNavigatedPayload {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

fn browser_window(app: &AppHandle) -> Result<tauri::Window, String> {
    app.get_window(BROWSER_WINDOW_LABEL)
        .ok_or_else(|| "browser window is not configured".to_string())
}

fn content_webview(app: &AppHandle) -> Option<tauri::Webview> {
    app.get_webview(BROWSER_CONTENT_LABEL)
}

fn chrome_webview(app: &AppHandle) -> Option<tauri::Webview> {
    app.get_webview(BROWSER_CHROME_LABEL)
}

fn parse_external_url(url: &str) -> Result<url::Url, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        return Err("url is empty".to_string());
    }
    trimmed
        .parse()
        .map_err(|err: url::ParseError| err.to_string())
}

fn window_inner_logical(window: &tauri::Window) -> Result<(f64, f64), String> {
    let scale = window.scale_factor().map_err(|err| err.to_string())?;
    let inner = window
        .inner_size()
        .map_err(|err| err.to_string())?
        .to_logical::<f64>(scale);
    Ok((inner.width, inner.height))
}

fn apply_layout(app: &AppHandle) -> Result<(), String> {
    let window = browser_window(app)?;
    let (width, height) = window_inner_logical(&window)?;
    let content_height = (height - CHROME_HEIGHT_LOGICAL).max(8.0);

    // Main webview is about:blank — shrink/hide so it does not paint a full-window white sheet.
    if let Some(main) = main_webview(app) {
        let _ = main.set_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(1.0, 1.0).into(),
        });
        let _ = main.hide();
    }

    if let Some(content) = content_webview(app) {
        content
            .set_bounds(Rect {
                position: LogicalPosition::new(0.0, CHROME_HEIGHT_LOGICAL).into(),
                size: LogicalSize::new(width, content_height).into(),
            })
            .map_err(|err| err.to_string())?;
        content.show().map_err(|err| err.to_string())?;
    }

    // Chrome last in layout pass so it stays above content for hit-testing.
    if let Some(chrome) = chrome_webview(app) {
        chrome
            .set_bounds(Rect {
                position: LogicalPosition::new(0.0, 0.0).into(),
                size: LogicalSize::new(width, CHROME_HEIGHT_LOGICAL).into(),
            })
            .map_err(|err| err.to_string())?;
        chrome.show().map_err(|err| err.to_string())?;
    }

    Ok(())
}

fn emit_navigated(app: &AppHandle, url: &str) {
    let payload = HumanBrowserNavigatedPayload {
        url: url.to_string(),
        title: None,
    };
    let _ = app.emit("human-browser-navigated", payload);
}

fn content_builder(app: &AppHandle) -> WebviewBuilder<tauri::Wry> {
    let app_nav = app.clone();
    let app_load = app.clone();
    WebviewBuilder::new(BROWSER_CONTENT_LABEL, WebviewUrl::External("about:blank".parse().unwrap()))
        .on_navigation(move |nav_url| {
            let href = nav_url.as_str().to_string();
            emit_navigated(&app_nav, &href);
            true
        })
        .on_page_load(move |_, payload| {
            use tauri::webview::PageLoadEvent;
            if payload.event() != PageLoadEvent::Finished {
                return;
            }
            let href = payload.url().as_str().to_string();
            emit_navigated(&app_load, &href);
        })
}

fn chrome_builder() -> WebviewBuilder<tauri::Wry> {
    WebviewBuilder::new(
        BROWSER_CHROME_LABEL,
        WebviewUrl::App("/popout/browser-chrome".into()),
    )
}

/// Create chrome + content child webviews and apply layout. Idempotent.
pub fn ensure_shell(app: &AppHandle) -> Result<(), String> {
    if SHELL_READY.load(Ordering::SeqCst) && content_webview(app).is_some() && chrome_webview(app).is_some() {
        return apply_layout(app);
    }

    let window = browser_window(app)?;
    let (width, height) = window_inner_logical(&window)?;
    let content_height = (height - CHROME_HEIGHT_LOGICAL).max(8.0);

    if content_webview(app).is_none() {
        window
            .add_child(
                content_builder(app),
                LogicalPosition::new(0.0, CHROME_HEIGHT_LOGICAL),
                LogicalSize::new(width, content_height),
            )
            .map_err(|err| err.to_string())?;
    }

    if chrome_webview(app).is_none() {
        window
            .add_child(
                chrome_builder(),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(width, CHROME_HEIGHT_LOGICAL),
            )
            .map_err(|err| err.to_string())?;
    }

    apply_layout(app)?;
    SHELL_READY.store(true, Ordering::SeqCst);
    Ok(())
}

pub fn prepare_browser_window(app: &AppHandle) -> Result<(), String> {
    ensure_shell(app)
}

pub fn on_browser_window_resized(app: &AppHandle) {
    if !SHELL_READY.load(Ordering::SeqCst) {
        return;
    }
    let _ = apply_layout(app);
}

#[tauri::command]
pub async fn human_browser_navigate(app: AppHandle, url: String) -> Result<(), String> {
    ensure_shell(&app)?;
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        if let Some(content) = content_webview(&app) {
            content
                .navigate(
                    "about:blank"
                        .parse()
                        .map_err(|err: url::ParseError| err.to_string())?,
                )
                .map_err(|err| err.to_string())?;
        }
        emit_navigated(&app, "about:blank");
        return Ok(());
    }
    let external = parse_external_url(trimmed)?;
    let content = content_webview(&app).ok_or_else(|| "browser content webview not ready".to_string())?;
    content
        .navigate(external)
        .map_err(|err| err.to_string())?;
    emit_navigated(&app, trimmed);
    Ok(())
}

#[tauri::command]
pub async fn human_browser_reload(app: AppHandle) -> Result<(), String> {
    let content = content_webview(&app).ok_or_else(|| "browser content webview not ready".to_string())?;
    content
        .eval("window.location.reload()")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_go_back(app: AppHandle) -> Result<(), String> {
    let content = content_webview(&app).ok_or_else(|| "browser content webview not ready".to_string())?;
    content
        .eval("window.history.back()")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_go_forward(app: AppHandle) -> Result<(), String> {
    let content = content_webview(&app).ok_or_else(|| "browser content webview not ready".to_string())?;
    content
        .eval("window.history.forward()")
        .map_err(|err| err.to_string())
}
