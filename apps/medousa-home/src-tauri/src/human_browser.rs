//! Human-first browser: Rust-managed native webviews.
//!
//! **Embedded (primary):** `main-browser-content` child on the main window, positioned
//! from the Web surface content pane. Chrome lives in Svelte (`HumanBrowserPanel`).
//!
//! **Pop-out (secondary):** `browser-content` + `browser-chrome` on the dedicated
//! browser window — kept for a future "Pop out" action.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use medousa_browser_lite::{markdown_from_html, search_response_from_ddg_html, FetchResult, SearchResponse};
use serde::{Deserialize, Serialize};
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, WebviewUrl};
use tokio::sync::oneshot;

const MAIN_WINDOW_LABEL: &str = "main";
const EMBED_CONTENT_LABEL: &str = "main-browser-content";

const BROWSER_WINDOW_LABEL: &str = "browser";
const BROWSER_CONTENT_LABEL: &str = "browser-content";
const BROWSER_CHROME_LABEL: &str = "browser-chrome";
const CHROME_HEIGHT_LOGICAL: f64 = 156.0;

static POPOUT_SHELL_READY: AtomicBool = AtomicBool::new(false);
static EMBED_READY: AtomicBool = AtomicBool::new(false);
static EMBED_VISIBLE: AtomicBool = AtomicBool::new(false);
/// When true the embedded webview was created with a mobile Safari user agent.
static EMBED_MOBILE_UA: AtomicBool = AtomicBool::new(false);
/// Set by the frontend when the mobile shell owns embed layout (blocks workshop resize reapply).
static MOBILE_SHELL_ACTIVE: AtomicBool = AtomicBool::new(false);
static LAST_EMBED_PLACEMENT: Mutex<Option<EmbedPlacement>> = Mutex::new(None);
static LAST_ACTIVE_URL: std::sync::OnceLock<Mutex<String>> = std::sync::OnceLock::new();
static SNAPSHOT_TX: Mutex<Option<oneshot::Sender<SnapshotReport>>> = Mutex::new(None);
static APP_HANDLE: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

fn active_url_lock() -> &'static Mutex<String> {
    LAST_ACTIVE_URL.get_or_init(|| Mutex::new(String::new()))
}

#[derive(Debug, Clone, Copy)]
enum EmbedPlacement {
    Workshop(EmbedLayoutParams),
    Mobile(EmbedMobileLayoutParams),
    Freeform(EmbedBounds),
}

/// Fallback mobile browser chrome when DOM bounds are unavailable (prefer `content_bounds`).
const MOBILE_BROWSER_CHROME_FALLBACK: f64 = 52.0;

/// Mobile Safari UA for responsive sites when the mobile shell is active (Tauri desktop resize).
const MOBILE_SAFARI_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";

/// Fix mobile UA viewport / safe-area so page content fills the native embed frame (mirrors iOS insets plugin).
const MOBILE_EMBED_FIX_JS: &str = r#"(function(){try{var d=document,h=d.head||d.documentElement,m=d.querySelector('meta[name="viewport"]');if(!m){m=d.createElement('meta');m.name='viewport';h.appendChild(m)}m.content='width=device-width,initial-scale=1,viewport-fit=cover';var s=d.getElementById('medousa-mobile-embed-fix');if(!s){s=d.createElement('style');s.id='medousa-mobile-embed-fix';s.textContent='html,body{min-height:100%;height:100%;margin:0;padding:0}body{padding-bottom:env(safe-area-inset-bottom,0)!important}';h.appendChild(s)}}catch(e){}})();"#;

fn inject_mobile_embed_fix(app: &AppHandle) {
    if !EMBED_MOBILE_UA.load(Ordering::SeqCst) {
        return;
    }
    if let Some(content) = embedded_content_webview(app) {
        let _ = content.eval(MOBILE_EMBED_FIX_JS);
    }
}
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanBrowserNavStatePayload {
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanBrowserLoadingPayload {
    loading: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindInPageResult {
    pub found: bool,
}

static NAV_STATE_TX: Mutex<Option<oneshot::Sender<HumanBrowserNavStatePayload>>> = Mutex::new(None);
static FIND_TX: Mutex<Option<oneshot::Sender<FindInPageResult>>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotReport {
    pub url: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHtmlDto {
    pub url: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMarkdownDto {
    pub url: String,
    pub title: String,
    pub markdown: String,
}

pub fn init_app_handle(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

pub fn app_handle() -> Option<AppHandle> {
    APP_HANDLE.get().cloned()
}

pub fn human_browser_active_url() -> String {
    active_url_lock().lock().expect("last active url").clone()
}

pub fn urls_match_for_snapshot(active: &str, requested: &str) -> bool {
    let active = active.trim();
    let requested = requested.trim();
    if active.is_empty() || requested.is_empty() || active == "about:blank" {
        return false;
    }
    if active == requested {
        return true;
    }
    let normalize = |value: &str| value.trim_end_matches('/').to_ascii_lowercase();
    normalize(active) == normalize(requested)
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

fn emit_loading(app: &AppHandle, loading: bool) {
    let _ = app.emit(
        "human-browser-loading",
        HumanBrowserLoadingPayload { loading },
    );
}

fn emit_nav_state(app: &AppHandle, can_go_back: bool, can_go_forward: bool) {
    let _ = app.emit(
        "human-browser-nav-state",
        HumanBrowserNavStatePayload {
            can_go_back,
            can_go_forward,
        },
    );
}

fn emit_navigated(app: &AppHandle, url: &str, title: Option<String>, favicon: Option<String>) {
    {
        let mut guard = active_url_lock().lock().expect("last active url");
        *guard = url.to_string();
    }
    let payload = HumanBrowserNavigatedPayload {
        url: url.to_string(),
        title,
        favicon,
    };
    let _ = app.emit("human-browser-navigated", payload);
}

fn probe_page_metadata(webview: &tauri::Webview, app: &AppHandle, url: &str) {
    let url_json = serde_json::to_string(url).unwrap_or_else(|_| "\"\"".to_string());
    let script = [
        "(function(){try{var u=",
        &url_json,
        ";var t=(document.title||'').trim();var fav=null;try{var link=document.querySelector('link[rel~=\"icon\"]');if(link&&link.href)fav=link.href;}catch(e){}var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;",
        "if(t)i.invoke('human_browser_report_title',{url:u,title:t});",
        "if(fav)i.invoke('human_browser_report_favicon',{url:u,favicon:fav});",
        "i.invoke('human_browser_report_nav_state',{canGoBack:window.history.length>1,canGoForward:false});",
        "}catch(e){}})();",
    ]
    .concat();
    let _ = webview.eval(&script);
    let _ = app;
}

fn content_builder(app: &AppHandle, label: &'static str, mobile_ua: bool) -> WebviewBuilder<tauri::Wry> {
    let app_nav = app.clone();
    let app_load = app.clone();
    let mut builder = WebviewBuilder::new(label, WebviewUrl::External("about:blank".parse().unwrap()))
        .on_navigation(move |nav_url| {
            let href = nav_url.as_str().to_string();
            emit_loading(&app_nav, true);
            emit_navigated(&app_nav, &href, None, None);
            true
        })
        .on_page_load(move |webview, payload| {
            use tauri::webview::PageLoadEvent;
            match payload.event() {
                PageLoadEvent::Started => emit_loading(&app_load, true),
                PageLoadEvent::Finished => {
                    emit_loading(&app_load, false);
                    let href = payload.url().as_str().to_string();
                    emit_navigated(&app_load, &href, None, None);
                    probe_page_metadata(&webview, &app_load, &href);
                    if mobile_ua {
                        let _ = webview.eval(MOBILE_EMBED_FIX_JS);
                    }
                }
            }
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

/// Mobile Web tab layout — prefers DOM-measured bounds when provided (fixes webview vs window size mismatch).
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
        y: 0.0,
        width: win_w.max(8.0),
        height: (win_h - MOBILE_BROWSER_CHROME_FALLBACK - bottom).max(8.0),
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
    if MOBILE_SHELL_ACTIVE.load(Ordering::SeqCst) {
        return Ok(());
    }
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
    MOBILE_SHELL_ACTIVE.store(true, Ordering::SeqCst);
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
    inject_mobile_embed_fix(app);
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
    // Mobile shell uses DOM-measured bounds from the frontend — never reapply workshop layout.
    if MOBILE_SHELL_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    let _ = reapply_embedded_placement(app);
}

#[tauri::command]
pub fn human_browser_set_mobile_shell_active(active: bool) {
    MOBILE_SHELL_ACTIVE.store(active, Ordering::SeqCst);
    if !active {
        // Desktop takeover — drop stale mobile placement so workshop resize reapply works.
        if let Ok(mut last) = LAST_EMBED_PLACEMENT.lock() {
            if matches!(*last, Some(EmbedPlacement::Mobile(_))) {
                *last = None;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedBoundsReadback {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub window_width: f64,
    pub window_height: f64,
}

#[tauri::command]
pub fn human_browser_embed_read_bounds(app: AppHandle) -> Result<EmbedBoundsReadback, String> {
    let window = workshop_window(&app)?;
    let (win_w, win_h) = window_inner_logical(&window)?;
    let content = embedded_content_webview(&app)
        .ok_or_else(|| "embedded content webview missing".to_string())?;
    let scale = window.scale_factor().map_err(|err| err.to_string())?;
    let actual = content.bounds().map_err(|err| err.to_string())?;
    let pos = actual.position.to_logical::<f64>(scale);
    let size = actual.size.to_logical::<f64>(scale);
    Ok(EmbedBoundsReadback {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
        window_width: win_w,
        window_height: win_h,
    })
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
        emit_navigated(&app, "about:blank", None, None);
        return Ok(());
    }
    let external = parse_external_url(trimmed)?;
    let content =
        embedded_content_webview(&app).ok_or_else(|| "embedded browser content not ready".to_string())?;
    content
        .navigate(external)
        .map_err(|err| err.to_string())?;
    emit_loading(&app, true);
    emit_navigated(&app, trimmed, None, None);
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

#[tauri::command]
pub fn human_browser_report_title(app: AppHandle, url: String, title: String) -> Result<(), String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    emit_navigated(&app, url.trim(), Some(trimmed.to_string()), None);
    Ok(())
}

const SNAPSHOT_CAPTURE_JS: &str = r#"(function(){
try{
  var html=document.documentElement?document.documentElement.outerHTML:"";
  var url=window.location.href||"";
  var i=window.__TAURI_INTERNALS__||window.__TAURI__;
  if(!i||!i.invoke)return;
  i.invoke("human_browser_report_snapshot",{url:url,html:html});
}catch(e){}
})();"#;

#[tauri::command]
pub fn human_browser_report_snapshot(payload: SnapshotReport) -> Result<(), String> {
    if let Some(tx) = SNAPSHOT_TX.lock().expect("snapshot").take() {
        let _ = tx.send(payload);
    }
    Ok(())
}

async fn capture_html(app: &AppHandle) -> Result<SnapshotReport, String> {
    let content =
        embedded_content_webview(app).ok_or_else(|| "embedded browser content not ready".to_string())?;
    let (tx, rx) = oneshot::channel();
    *SNAPSHOT_TX.lock().expect("snapshot") = Some(tx);
    content
        .eval(SNAPSHOT_CAPTURE_JS)
        .map_err(|err| err.to_string())?;
    tokio::time::timeout(Duration::from_secs(8), rx)
        .await
        .map_err(|_| "snapshot timed out waiting for page content".to_string())?
        .map_err(|_| "snapshot channel closed".to_string())
}

#[tauri::command]
pub async fn human_browser_snapshot_html(app: AppHandle) -> Result<SnapshotHtmlDto, String> {
    let report = capture_html(&app).await?;
    Ok(SnapshotHtmlDto {
        url: report.url,
        html: report.html,
    })
}

#[tauri::command]
pub async fn human_browser_snapshot_markdown(
    app: AppHandle,
    max_chars: Option<usize>,
) -> Result<SnapshotMarkdownDto, String> {
    let report = capture_html(&app).await?;
    let fetched = markdown_from_html(&report.html, &report.url, max_chars.unwrap_or(4000));
    Ok(SnapshotMarkdownDto {
        url: fetched.url,
        title: fetched.title,
        markdown: fetched.markdown,
    })
}

#[tauri::command]
pub async fn human_browser_snapshot_search(
    app: AppHandle,
    query: String,
    max_results: Option<usize>,
) -> Result<SearchResponse, String> {
    let report = capture_html(&app).await?;
    Ok(search_response_from_ddg_html(
        &report.html,
        &report.url,
        &query,
        max_results.unwrap_or(8),
    ))
}

pub async fn snapshot_markdown_for_url(
    app: &AppHandle,
    url: &str,
    max_chars: usize,
) -> Result<FetchResult, String> {
    let active = human_browser_active_url();
    if !urls_match_for_snapshot(&active, url) {
        return Err(format!(
            "human browser active url mismatch: active={active} requested={url}"
        ));
    }
    let report = capture_html(app).await?;
    Ok(markdown_from_html(&report.html, &report.url, max_chars))
}

#[tauri::command]
pub fn human_browser_report_nav_state(payload: HumanBrowserNavStatePayload) -> Result<(), String> {
    if let Some(tx) = NAV_STATE_TX.lock().expect("nav state").take() {
        let _ = tx.send(payload.clone());
    }
    if let Some(app) = app_handle() {
        emit_nav_state(&app, payload.can_go_back, payload.can_go_forward);
    }
    Ok(())
}

#[tauri::command]
pub fn human_browser_report_favicon(url: String, favicon: String) -> Result<(), String> {
    let trimmed = favicon.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if let Some(app) = app_handle() {
        emit_navigated(&app, url.trim(), None, Some(trimmed.to_string()));
    }
    Ok(())
}

#[tauri::command]
pub fn human_browser_report_find_result(found: bool) -> Result<(), String> {
    if let Some(tx) = FIND_TX.lock().expect("find").take() {
        let _ = tx.send(FindInPageResult { found });
    }
    Ok(())
}

#[tauri::command]
pub async fn human_browser_stop(app: AppHandle) -> Result<(), String> {
    let content =
        embedded_content_webview(&app).ok_or_else(|| "embedded browser content not ready".to_string())?;
    emit_loading(&app, false);
    content
        .eval("try{window.stop();}catch(e){}")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_query_nav_state(app: AppHandle) -> Result<HumanBrowserNavStatePayload, String> {
    let content =
        embedded_content_webview(&app).ok_or_else(|| "embedded browser content not ready".to_string())?;
    let (tx, rx) = oneshot::channel();
    *NAV_STATE_TX.lock().expect("nav state") = Some(tx);
    content
        .eval(
            r#"(function(){try{var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;i.invoke('human_browser_report_nav_state',{canGoBack:window.history.length>1,canGoForward:false});}catch(e){}})();"#,
        )
        .map_err(|err| err.to_string())?;
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .map_err(|_| "navigation state query timed out".to_string())?
        .map_err(|_| "navigation state channel closed".to_string())
}

#[tauri::command]
pub async fn human_browser_find_in_page(
    app: AppHandle,
    query: String,
    forward: Option<bool>,
) -> Result<FindInPageResult, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(FindInPageResult { found: false });
    }
    let content =
        embedded_content_webview(&app).ok_or_else(|| "embedded browser content not ready".to_string())?;
    let (tx, rx) = oneshot::channel();
    *FIND_TX.lock().expect("find") = Some(tx);
    let forward_lit = if forward.unwrap_or(true) { "true" } else { "false" };
    let query_json = serde_json::to_string(trimmed).map_err(|err| err.to_string())?;
    let script = format!(
        r#"(function(){{try{{var q={query_json};var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;var found=window.find(q,false,{forward_lit},true,false,true,false);i.invoke('human_browser_report_find_result',{{found:!!found}});}}catch(e){{}}}})();"#
    );
    content.eval(&script).map_err(|err| err.to_string())?;
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .map_err(|_| "find in page timed out".to_string())?
        .map_err(|_| "find in page channel closed".to_string())
}
