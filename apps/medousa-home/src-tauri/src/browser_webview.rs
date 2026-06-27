//! Embedded native browsing webview (WKWebView / WebView2) inside the main window.

use std::sync::Mutex;

use serde::Deserialize;
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, WebviewUrl};

const BROWSER_WEBVIEW_LABEL: &str = "medousa-browser-pane";
const MAIN_WINDOW_LABEL: &str = "main";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWebviewBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone)]
struct SyncState {
    last_url: Option<String>,
}

static STATE: Mutex<SyncState> = Mutex::new(SyncState { last_url: None });

fn main_window(app: &AppHandle) -> Result<tauri::Window, String> {
    app.get_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window not found".to_string())
}

fn child_webview(app: &AppHandle) -> Option<tauri::Webview> {
    app.get_webview(BROWSER_WEBVIEW_LABEL)
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

/// Child webviews share the main webview's top-left viewport coordinate system.
/// Pass through DOM `getBoundingClientRect` values without window chrome offsets.
fn normalize_bounds(bounds: BrowserWebviewBounds) -> BrowserWebviewBounds {
    BrowserWebviewBounds {
        x: bounds.x.max(0.0),
        y: bounds.y.max(0.0),
        width: bounds.width.max(8.0),
        height: bounds.height.max(8.0),
    }
}

fn apply_bounds(webview: &tauri::Webview, bounds: BrowserWebviewBounds) -> Result<(), String> {
    webview
        .set_bounds(Rect {
            position: LogicalPosition::new(bounds.x, bounds.y).into(),
            size: LogicalSize::new(bounds.width, bounds.height).into(),
        })
        .map_err(|err| err.to_string())
}

fn remember_native_url(url: &str) {
    STATE
        .lock()
        .expect("browser webview state")
        .last_url = Some(url.trim().to_string());
}

#[tauri::command]
pub async fn browser_webview_sync(
    app: AppHandle,
    bounds: BrowserWebviewBounds,
    visible: bool,
    url: Option<String>,
) -> Result<(), String> {
    let window = main_window(&app)?;
    let bounds = normalize_bounds(bounds);

    if let Some(webview) = child_webview(&app) {
        if !visible || bounds.width < 8.0 || bounds.height < 8.0 {
            let _ = webview.hide();
            return Ok(());
        }
        apply_bounds(&webview, bounds)?;
        webview.show().map_err(|err| err.to_string())?;
        return Ok(());
    }

    if !visible || bounds.width < 8.0 || bounds.height < 8.0 {
        return Ok(());
    }

    let initial_url = url
        .as_deref()
        .ok_or_else(|| "url required to create browser webview".to_string())?;
    let external = parse_external_url(initial_url)?;

    let app_handle = app.clone();
    let builder = WebviewBuilder::new(BROWSER_WEBVIEW_LABEL, WebviewUrl::External(external))
        .on_navigation(move |nav_url| {
            let href = nav_url.as_str().to_string();
            remember_native_url(&href);
            let _ = app_handle.emit("browser-native-navigated", href);
            true
        });

    let webview = window
        .add_child(
            builder,
            LogicalPosition::new(bounds.x, bounds.y),
            LogicalSize::new(bounds.width, bounds.height),
        )
        .map_err(|err| err.to_string())?;

    remember_native_url(initial_url);
    webview.show().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn browser_webview_navigate(app: AppHandle, url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        return Ok(());
    }

    if STATE
        .lock()
        .expect("browser webview state")
        .last_url
        .as_deref()
        == Some(trimmed)
    {
        return Ok(());
    }

    let webview = child_webview(&app).ok_or_else(|| "browser webview not mounted".to_string())?;
    let external = parse_external_url(trimmed)?;
    webview
        .navigate(external)
        .map_err(|err| err.to_string())?;
    remember_native_url(trimmed);
    Ok(())
}

#[tauri::command]
pub async fn browser_webview_reload(app: AppHandle) -> Result<(), String> {
    let webview = child_webview(&app).ok_or_else(|| "browser webview not mounted".to_string())?;
    webview
        .eval("window.location.reload()")
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn browser_webview_go_back(app: AppHandle) -> Result<(), String> {
    let webview = child_webview(&app).ok_or_else(|| "browser webview not mounted".to_string())?;
    webview
        .eval("window.history.back()")
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn browser_webview_go_forward(app: AppHandle) -> Result<(), String> {
    let webview = child_webview(&app).ok_or_else(|| "browser webview not mounted".to_string())?;
    webview
        .eval("window.history.forward()")
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn browser_webview_hide(app: AppHandle) -> Result<(), String> {
    if let Some(webview) = child_webview(&app) {
        webview.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}
