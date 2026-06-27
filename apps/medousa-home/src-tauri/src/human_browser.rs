//! Human-first browser: Rust-managed native webviews.
//!
//! **Embedded (primary):** `main-browser-content` child on the main window, positioned
//! from the Web surface content pane. Chrome lives in Svelte (`HumanBrowserPanel`).
//!
//! **Pop-out (secondary):** `browser-content` + `browser-chrome` on the dedicated
//! browser window — kept for a future "Pop out" action.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, WebviewUrl};

const MAIN_WINDOW_LABEL: &str = "main";
const EMBED_CONTENT_LABEL: &str = "main-browser-content";

const BROWSER_WINDOW_LABEL: &str = "browser";
const BROWSER_CONTENT_LABEL: &str = "browser-content";
const BROWSER_CHROME_LABEL: &str = "browser-chrome";
const CHROME_HEIGHT_LOGICAL: f64 = 132.0;

static POPOUT_SHELL_READY: AtomicBool = AtomicBool::new(false);
static EMBED_READY: AtomicBool = AtomicBool::new(false);
static EMBED_VISIBLE: AtomicBool = AtomicBool::new(false);
/// When true the embedded webview was created with a mobile Safari user agent.
static EMBED_MOBILE_UA: AtomicBool = AtomicBool::new(false);
static LAST_EMBED_PLACEMENT: Mutex<Option<EmbedPlacement>> = Mutex::new(None);

#[derive(Debug, Clone, Copy)]
enum EmbedPlacement {
    Workshop(EmbedLayoutParams),
    Mobile(EmbedMobileLayoutParams),
    Freeform(EmbedBounds),
}

/// Mobile Web tab toolbar — must match `h-[52px]` on BrowserPanel toolbar row.
const MOBILE_WEB_CHROME_HEIGHT: f64 = 52.0;

/// Mobile Safari UA for responsive sites when the mobile shell is active (Tauri desktop resize).
const MOBILE_SAFARI_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
/// Default bottom tab bar — matches `--mobile-bottom-chrome-height` fallback (5.5rem).
const MOBILE_BOTTOM_CHROME_DEFAULT: f64 = 88.0;

/// Left nav rail width — must match `.workshop-icon-rail` (`w-[52px]`).
const NAV_RAIL_WIDTH: f64 = 52.0;
/// Collapsed activity strip — must match `ACTIVITY_STRIP` in desktopRails.ts.
const ACTIVITY_STRIP_WIDTH: f64 = 28.0;
/// Status footer — must match `.workshop-status` `h-8`.
const STATUS_BAR_HEIGHT: f64 = 32.0;
/// Work rail — must match `WorkRail.svelte` `h-24`.
const WORK_RAIL_HEIGHT: f64 = 96.0;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedMobileLayoutParams {
    pub bottom_chrome_height: f64,
    /// When set, use DOM-measured content pane bounds (from `[data-browser-surface]`).
    pub content_bounds: Option<EmbedBoundsDto>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedLayoutParams {
    pub activity_width: f64,
    pub activity_collapsed: bool,
    pub work_rail_visible: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedBoundsDto {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy)]
struct EmbedBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl From<EmbedBoundsDto> for EmbedBounds {
    fn from(value: EmbedBoundsDto) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanBrowserNavigatedPayload {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

fn popout_main_webview(app: &AppHandle) -> Option<tauri::Webview> {
    app.get_webview(BROWSER_WINDOW_LABEL)
}

fn popout_window(app: &AppHandle) -> Result<tauri::Window, String> {
    app.get_window(BROWSER_WINDOW_LABEL)
        .ok_or_else(|| "browser window is not configured".to_string())
}

fn popout_content_webview(app: &AppHandle) -> Option<tauri::Webview> {
    app.get_webview(BROWSER_CONTENT_LABEL)
}

fn popout_chrome_webview(app: &AppHandle) -> Option<tauri::Webview> {
    app.get_webview(BROWSER_CHROME_LABEL)
}

fn workshop_window(app: &AppHandle) -> Result<tauri::Window, String> {
    app.get_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not configured".to_string())
}

fn embedded_content_webview(app: &AppHandle) -> Option<tauri::Webview> {
    app.get_webview(EMBED_CONTENT_LABEL)
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

fn emit_navigated(app: &AppHandle, url: &str) {
    let payload = HumanBrowserNavigatedPayload {
        url: url.to_string(),
        title: None,
    };
    let _ = app.emit("human-browser-navigated", payload);
}

fn content_builder(app: &AppHandle, label: &'static str, mobile_ua: bool) -> WebviewBuilder<tauri::Wry> {
    let app_nav = app.clone();
    let app_load = app.clone();
    let mut builder = WebviewBuilder::new(label, WebviewUrl::External("about:blank".parse().unwrap()))
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
        });
    if mobile_ua {
        builder = builder.user_agent(MOBILE_SAFARI_UA);
    }
    builder
}

fn chrome_builder() -> WebviewBuilder<tauri::Wry> {
    WebviewBuilder::new(
        BROWSER_CHROME_LABEL,
        WebviewUrl::App("/popout/browser-chrome".into()),
    )
}

fn default_embed_layout() -> EmbedLayoutParams {
    EmbedLayoutParams {
        activity_width: 288.0,
        activity_collapsed: false,
        work_rail_visible: false,
    }
}

/// Fixed Rust layout for the embedded pane — mirrors pop-out `apply_popout_layout`.
fn compute_embedded_bounds(
    window: &tauri::Window,
    params: EmbedLayoutParams,
) -> Result<EmbedBounds, String> {
    let (win_w, win_h) = window_inner_logical(window)?;
    let activity_w = if params.activity_collapsed {
        ACTIVITY_STRIP_WIDTH
    } else {
        params.activity_width.max(ACTIVITY_STRIP_WIDTH)
    };
    let bottom_chrome =
        STATUS_BAR_HEIGHT + if params.work_rail_visible { WORK_RAIL_HEIGHT } else { 0.0 };

    Ok(EmbedBounds {
        x: NAV_RAIL_WIDTH,
        y: CHROME_HEIGHT_LOGICAL,
        width: (win_w - NAV_RAIL_WIDTH - activity_w).max(8.0),
        height: (win_h - CHROME_HEIGHT_LOGICAL - bottom_chrome).max(8.0),
    })
}

fn default_mobile_embed_layout() -> EmbedMobileLayoutParams {
    EmbedMobileLayoutParams {
        bottom_chrome_height: MOBILE_BOTTOM_CHROME_DEFAULT,
        content_bounds: None,
    }
}

/// Drop the embedded webview so it can be recreated (e.g. when switching mobile/desktop UA).
fn reset_embedded_content(app: &AppHandle) -> Result<(), String> {
    if let Some(content) = embedded_content_webview(app) {
        content.close().map_err(|err| err.to_string())?;
    }
    EMBED_READY.store(false, Ordering::SeqCst);
    Ok(())
}

/// Ensure the embed uses a mobile Safari user agent; recreate the webview when switching from desktop.
fn ensure_embedded_mobile_profile(app: &AppHandle) -> Result<bool, String> {
    if EMBED_MOBILE_UA.load(Ordering::SeqCst) && embedded_content_webview(app).is_some() {
        return Ok(false);
    }
    reset_embedded_content(app)?;
    EMBED_MOBILE_UA.store(true, Ordering::SeqCst);
    Ok(true)
}

/// Ensure the embed uses the default desktop user agent; recreate when switching from mobile.
fn ensure_embedded_desktop_profile(app: &AppHandle) -> Result<bool, String> {
    if !EMBED_MOBILE_UA.load(Ordering::SeqCst) && embedded_content_webview(app).is_some() {
        return Ok(false);
    }
    reset_embedded_content(app)?;
    EMBED_MOBILE_UA.store(false, Ordering::SeqCst);
    Ok(true)
}

/// Fixed Rust layout for mobile Web tab — prefers DOM-measured bounds when provided.
fn compute_mobile_embedded_bounds(
    window: &tauri::Window,
    params: EmbedMobileLayoutParams,
) -> Result<EmbedBounds, String> {
    if let Some(measured) = params.content_bounds {
        return Ok(EmbedBounds {
            x: measured.x,
            y: measured.y,
            width: measured.width.max(8.0),
            height: measured.height.max(8.0),
        });
    }

    let (win_w, win_h) = window_inner_logical(window)?;
    let bottom = params.bottom_chrome_height.max(0.0);

    Ok(EmbedBounds {
        x: 0.0,
        y: MOBILE_WEB_CHROME_HEIGHT,
        width: win_w.max(8.0),
        height: (win_h - MOBILE_WEB_CHROME_HEIGHT - bottom).max(8.0),
    })
}

fn apply_embedded_bounds(app: &AppHandle, bounds: EmbedBounds) -> Result<(), String> {
    let width = bounds.width.max(8.0);
    let height = bounds.height.max(8.0);
    if let Some(content) = embedded_content_webview(app) {
        content
            .set_bounds(Rect {
                position: LogicalPosition::new(bounds.x, bounds.y).into(),
                size: LogicalSize::new(width, height).into(),
            })
            .map_err(|err| err.to_string())?;
        if EMBED_VISIBLE.load(Ordering::SeqCst) {
            content.show().map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn apply_embedded_layout(app: &AppHandle, params: EmbedLayoutParams) -> Result<(), String> {
    ensure_embedded_desktop_profile(app)?;
    ensure_embedded_content(app)?;
    let window = workshop_window(app)?;
    let bounds = compute_embedded_bounds(&window, params)?;
    if let Ok(mut last) = LAST_EMBED_PLACEMENT.lock() {
        *last = Some(EmbedPlacement::Workshop(params));
    }
    apply_embedded_bounds(app, bounds)?;
    EMBED_VISIBLE.store(true, Ordering::SeqCst);
    if let Some(content) = embedded_content_webview(app) {
        content.show().map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn apply_embedded_mobile_layout(
    app: &AppHandle,
    params: EmbedMobileLayoutParams,
) -> Result<bool, String> {
    let recreated = ensure_embedded_mobile_profile(app)?;
    ensure_embedded_content(app)?;
    let window = workshop_window(app)?;
    let bounds = compute_mobile_embedded_bounds(&window, params)?;
    if let Ok(mut last) = LAST_EMBED_PLACEMENT.lock() {
        *last = Some(EmbedPlacement::Mobile(params));
    }
    apply_embedded_bounds(app, bounds)?;
    EMBED_VISIBLE.store(true, Ordering::SeqCst);
    if let Some(content) = embedded_content_webview(app) {
        content.show().map_err(|err| err.to_string())?;
    }
    Ok(recreated)
}

fn apply_embedded_freeform(app: &AppHandle, bounds: EmbedBounds) -> Result<(), String> {
    ensure_embedded_content(app)?;
    if let Ok(mut last) = LAST_EMBED_PLACEMENT.lock() {
        *last = Some(EmbedPlacement::Freeform(bounds));
    }
    apply_embedded_bounds(app, bounds)
}

fn reapply_embedded_placement(app: &AppHandle) -> Result<(), String> {
    let Some(placement) = LAST_EMBED_PLACEMENT.lock().ok().and_then(|guard| *guard) else {
        return Ok(());
    };
    match placement {
        EmbedPlacement::Workshop(params) => apply_embedded_layout(app, params),
        EmbedPlacement::Mobile(params) => apply_embedded_mobile_layout(app, params).map(|_| ()),
        EmbedPlacement::Freeform(bounds) => apply_embedded_bounds(app, bounds),
    }
}

/// Create the embedded content webview on the main window if needed.
pub fn ensure_embedded_content(app: &AppHandle) -> Result<(), String> {
    if EMBED_READY.load(Ordering::SeqCst) && embedded_content_webview(app).is_some() {
        return Ok(());
    }
    if embedded_content_webview(app).is_none() {
        EMBED_READY.store(false, Ordering::SeqCst);
    }

    if embedded_content_webview(app).is_some() {
        EMBED_READY.store(true, Ordering::SeqCst);
        return Ok(());
    }

    let window = workshop_window(app)?;
    let initial_bounds = match LAST_EMBED_PLACEMENT.lock().ok().and_then(|guard| *guard) {
        Some(EmbedPlacement::Freeform(bounds)) => bounds,
        Some(EmbedPlacement::Workshop(params)) => compute_embedded_bounds(&window, params)?,
        Some(EmbedPlacement::Mobile(params)) => compute_mobile_embedded_bounds(&window, params)?,
        None => compute_embedded_bounds(&window, default_embed_layout())?,
    };

    window
        .add_child(
            content_builder(
                app,
                EMBED_CONTENT_LABEL,
                EMBED_MOBILE_UA.load(Ordering::SeqCst),
            ),
            LogicalPosition::new(initial_bounds.x, initial_bounds.y),
            LogicalSize::new(initial_bounds.width, initial_bounds.height),
        )
        .map_err(|err| err.to_string())?;

    EMBED_READY.store(true, Ordering::SeqCst);

    if EMBED_VISIBLE.load(Ordering::SeqCst) {
        apply_embedded_bounds(app, initial_bounds)?;
    } else if let Some(content) = embedded_content_webview(app) {
        let _ = content.hide();
    }

    Ok(())
}

#[tauri::command]
pub fn human_browser_embed_apply_layout(
    app: AppHandle,
    params: EmbedLayoutParams,
) -> Result<(), String> {
    apply_embedded_layout(&app, params)
}

#[tauri::command]
pub fn human_browser_embed_apply_mobile_layout(
    app: AppHandle,
    params: EmbedMobileLayoutParams,
) -> Result<bool, String> {
    apply_embedded_mobile_layout(&app, params)
}

#[tauri::command]
pub fn human_browser_embed_set_bounds(
    app: AppHandle,
    bounds: EmbedBoundsDto,
) -> Result<(), String> {
    apply_embedded_freeform(&app, bounds.into())
}

#[tauri::command]
pub fn human_browser_embed_show(app: AppHandle) -> Result<(), String> {
    ensure_embedded_content(&app)?;
    EMBED_VISIBLE.store(true, Ordering::SeqCst);
    if LAST_EMBED_PLACEMENT
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .is_some()
    {
        reapply_embedded_placement(&app)?;
    } else if let Some(content) = embedded_content_webview(&app) {
        content.show().map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn on_main_window_resized(app: &AppHandle) {
    if !EMBED_VISIBLE.load(Ordering::SeqCst) {
        return;
    }
    // Mobile uses DOM-measured bounds from the frontend; skip stale Rust reapply.
    if matches!(
        LAST_EMBED_PLACEMENT.lock().ok().and_then(|guard| *guard),
        Some(EmbedPlacement::Mobile(_))
    ) {
        return;
    }
    let _ = reapply_embedded_placement(app);
}

#[tauri::command]
pub fn human_browser_embed_hide(app: AppHandle) -> Result<(), String> {
    EMBED_VISIBLE.store(false, Ordering::SeqCst);
    if let Ok(mut last) = LAST_EMBED_PLACEMENT.lock() {
        *last = None;
    }
    if let Some(content) = embedded_content_webview(&app) {
        content.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn apply_popout_layout(app: &AppHandle) -> Result<(), String> {
    let window = popout_window(app)?;
    let (width, height) = window_inner_logical(&window)?;
    let content_height = (height - CHROME_HEIGHT_LOGICAL).max(8.0);

    if let Some(main) = popout_main_webview(app) {
        let _ = main.set_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(1.0, 1.0).into(),
        });
        let _ = main.hide();
    }

    if let Some(content) = popout_content_webview(app) {
        content
            .set_bounds(Rect {
                position: LogicalPosition::new(0.0, CHROME_HEIGHT_LOGICAL).into(),
                size: LogicalSize::new(width, content_height).into(),
            })
            .map_err(|err| err.to_string())?;
        content.show().map_err(|err| err.to_string())?;
    }

    if let Some(chrome) = popout_chrome_webview(app) {
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

/// Create pop-out chrome + content child webviews. Idempotent.
pub fn ensure_popout_shell(app: &AppHandle) -> Result<(), String> {
    if POPOUT_SHELL_READY.load(Ordering::SeqCst)
        && (popout_content_webview(app).is_none() || popout_chrome_webview(app).is_none())
    {
        POPOUT_SHELL_READY.store(false, Ordering::SeqCst);
    }

    if POPOUT_SHELL_READY.load(Ordering::SeqCst)
        && popout_content_webview(app).is_some()
        && popout_chrome_webview(app).is_some()
    {
        return apply_popout_layout(app);
    }

    let window = popout_window(app)?;
    let (width, height) = window_inner_logical(&window)?;
    let content_height = (height - CHROME_HEIGHT_LOGICAL).max(8.0);

    if popout_content_webview(app).is_none() {
        window
            .add_child(
                content_builder(app, BROWSER_CONTENT_LABEL, false),
                LogicalPosition::new(0.0, CHROME_HEIGHT_LOGICAL),
                LogicalSize::new(width, content_height),
            )
            .map_err(|err| err.to_string())?;
    }

    if popout_chrome_webview(app).is_none() {
        window
            .add_child(
                chrome_builder(),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(width, CHROME_HEIGHT_LOGICAL),
            )
            .map_err(|err| err.to_string())?;
    }

    apply_popout_layout(app)?;
    POPOUT_SHELL_READY.store(true, Ordering::SeqCst);
    Ok(())
}

pub fn prepare_browser_window(app: &AppHandle) -> Result<(), String> {
    ensure_popout_shell(app)
}

pub fn on_browser_window_resized(app: &AppHandle) {
    if !POPOUT_SHELL_READY.load(Ordering::SeqCst) {
        return;
    }
    let _ = apply_popout_layout(app);
}

#[tauri::command]
pub async fn human_browser_navigate(app: AppHandle, url: String) -> Result<(), String> {
    ensure_embedded_content(&app)?;
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        if let Some(content) = embedded_content_webview(&app) {
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
    let content =
        embedded_content_webview(&app).ok_or_else(|| "embedded browser content not ready".to_string())?;
    content
        .navigate(external)
        .map_err(|err| err.to_string())?;
    emit_navigated(&app, trimmed);
    Ok(())
}

#[tauri::command]
pub async fn human_browser_reload(app: AppHandle) -> Result<(), String> {
    let content =
        embedded_content_webview(&app).ok_or_else(|| "embedded browser content not ready".to_string())?;
    content
        .eval("window.location.reload()")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_go_back(app: AppHandle) -> Result<(), String> {
    let content =
        embedded_content_webview(&app).ok_or_else(|| "embedded browser content not ready".to_string())?;
    content
        .eval("window.history.back()")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_go_forward(app: AppHandle) -> Result<(), String> {
    let content =
        embedded_content_webview(&app).ok_or_else(|| "embedded browser content not ready".to_string())?;
    content
        .eval("window.history.forward()")
        .map_err(|err| err.to_string())
}
