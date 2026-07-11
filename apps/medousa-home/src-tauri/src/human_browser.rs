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
use tauri::webview::{Color, NewWindowResponse, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, WebviewUrl};
use tokio::sync::oneshot;

const MAIN_WINDOW_LABEL: &str = "main";
const EMBED_CONTENT_LABEL: &str = "main-browser-content";
/// Shell surface background — aligns WKWebView under-page compositing with the workshop chrome.
const EMBED_SURFACE_COLOR: Color = Color(12, 14, 18, 255);

const BROWSER_WINDOW_LABEL: &str = "browser";
const BROWSER_CONTENT_LABEL: &str = "browser-content";
const BROWSER_CHROME_LABEL: &str = "browser-chrome";
const CHROME_HEIGHT_LOGICAL: f64 = 156.0;
/// Pop-out chrome strip — must match `h-[132px]` in `popout/browser-chrome/+page.svelte`.
const POPOUT_CHROME_HEIGHT_LOGICAL: f64 = 132.0;

static POPOUT_SHELL_READY: AtomicBool = AtomicBool::new(false);
static EMBED_READY: AtomicBool = AtomicBool::new(false);
static EMBED_VISIBLE: AtomicBool = AtomicBool::new(false);
/// When true the embedded webview was created with a mobile Safari user agent.
static EMBED_MOBILE_UA: AtomicBool = AtomicBool::new(false);
/// Set by the frontend when the mobile shell owns embed layout (blocks workshop resize reapply).
static MOBILE_SHELL_ACTIVE: AtomicBool = AtomicBool::new(false);
static LAST_EMBED_PLACEMENT: Mutex<Option<EmbedPlacement>> = Mutex::new(None);
/// Last successful macOS title-bar inset. `with_webview` can miss on first create;
/// falling back to (0,0) shifts child embeds up under the OS title bar.
static LAST_VIEWPORT_INSET: Mutex<Option<(f64, f64)>> = Mutex::new(None);
/// URL queued when navigate runs before the compositor has created/sized the embed.
static LAST_EMBED_ACTIVE_URL: std::sync::OnceLock<Mutex<String>> = std::sync::OnceLock::new();
static LAST_POPOUT_ACTIVE_URL: std::sync::OnceLock<Mutex<String>> = std::sync::OnceLock::new();
static EMBED_ACTIVE_TAB_ID: Mutex<Option<String>> = Mutex::new(None);
static POPOUT_ACTIVE_TAB_ID: Mutex<Option<String>> = Mutex::new(None);
static EMBED_TAB_IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static POPOUT_TAB_IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SNAPSHOT_TX: Mutex<Option<oneshot::Sender<SnapshotReport>>> = Mutex::new(None);
static APP_HANDLE: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserSurface {
    Embed,
    Popout,
}

impl BrowserSurface {
    fn as_str(self) -> &'static str {
        match self {
            Self::Embed => "embed",
            Self::Popout => "popout",
        }
    }
}

fn surface_url_lock(surface: BrowserSurface) -> &'static Mutex<String> {
    match surface {
        BrowserSurface::Embed => {
            LAST_EMBED_ACTIVE_URL.get_or_init(|| Mutex::new(String::new()))
        }
        BrowserSurface::Popout => {
            LAST_POPOUT_ACTIVE_URL.get_or_init(|| Mutex::new(String::new()))
        }
    }
}

fn active_tab_id_lock(surface: BrowserSurface) -> &'static Mutex<Option<String>> {
    match surface {
        BrowserSurface::Embed => &EMBED_ACTIVE_TAB_ID,
        BrowserSurface::Popout => &POPOUT_ACTIVE_TAB_ID,
    }
}

fn tab_ids_lock(surface: BrowserSurface) -> &'static Mutex<Vec<String>> {
    match surface {
        BrowserSurface::Embed => &EMBED_TAB_IDS,
        BrowserSurface::Popout => &POPOUT_TAB_IDS,
    }
}

fn sanitize_tab_id(tab_id: &str) -> String {
    let sanitized: String = tab_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "tab".to_string()
    } else {
        sanitized.chars().take(48).collect()
    }
}

fn tab_webview_label(surface: BrowserSurface, tab_id: &str) -> String {
    let prefix = match surface {
        BrowserSurface::Embed => "embed-tab-",
        BrowserSurface::Popout => "popout-tab-",
    };
    format!("{}{}", prefix, sanitize_tab_id(tab_id))
}

fn tab_webview(app: &AppHandle, surface: BrowserSurface, tab_id: &str) -> Option<tauri::Webview> {
    app.get_webview(&tab_webview_label(surface, tab_id))
}

fn active_tab_id(surface: BrowserSurface) -> Option<String> {
    active_tab_id_lock(surface)
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

fn active_tab_webview(app: &AppHandle, surface: BrowserSurface) -> Option<tauri::Webview> {
    let tab_id = active_tab_id(surface)?;
    tab_webview(app, surface, &tab_id)
}

fn register_tab_id(surface: BrowserSurface, tab_id: &str) {
    if let Ok(mut ids) = tab_ids_lock(surface).lock() {
        if !ids.iter().any(|id| id == tab_id) {
            ids.push(tab_id.to_string());
        }
    }
}

fn unregister_tab_id(surface: BrowserSurface, tab_id: &str) {
    if let Ok(mut ids) = tab_ids_lock(surface).lock() {
        ids.retain(|id| id != tab_id);
    }
}

fn hide_tab_webviews(app: &AppHandle, surface: BrowserSurface, except: Option<&str>) {
    let ids = tab_ids_lock(surface)
        .lock()
        .ok()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    for id in ids {
        if except == Some(id.as_str()) {
            continue;
        }
        if let Some(webview) = tab_webview(app, surface, &id) {
            let _ = webview.hide();
        }
    }
}

fn close_legacy_content_webview(app: &AppHandle, label: &str) {
    if let Some(content) = app.get_webview(label) {
        let _ = content.close();
    }
}

fn close_all_tab_webviews(app: &AppHandle, surface: BrowserSurface) {
    let ids = tab_ids_lock(surface)
        .lock()
        .ok()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    for id in ids {
        if let Some(webview) = tab_webview(app, surface, &id) {
            let _ = webview.close();
        }
    }
    if let Ok(mut ids) = tab_ids_lock(surface).lock() {
        ids.clear();
    }
    if let Ok(mut active) = active_tab_id_lock(surface).lock() {
        *active = None;
    }
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

/// Reset default document margins in the embedded page; clipping is handled natively (NSView clipsToBounds).
const DESKTOP_EMBED_FILL_JS: &str = r#"(function(){try{var s=document.getElementById('medousa-desktop-embed-fill');if(!s){s=document.createElement('style');s.id='medousa-desktop-embed-fill';(document.head||document.documentElement).appendChild(s)}s.textContent='html,body{margin:0;padding:0;background:#0c0e12}';}catch(e){}})();"#;

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
/// Work rail — retired from layout (in-motion lives in StatusBar peek). Kept at 0.
const WORK_RAIL_HEIGHT: f64 = 0.0;

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
    /// Measured chrome bottom in shell viewport (`getBoundingClientRect().bottom`).
    pub content_top: Option<f64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_id: Option<String>,
    #[serde(default = "default_embed_surface")]
    pub surface: String,
}

fn default_embed_surface() -> String {
    "embed".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanBrowserNavStatePayload {
    pub can_go_back: bool,
    pub can_go_forward: bool,
    #[serde(default = "default_embed_surface")]
    pub surface: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanBrowserLoadingPayload {
    loading: bool,
    #[serde(default = "default_embed_surface")]
    surface: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanBrowserNewWindowPayload {
    url: String,
    #[serde(default = "default_embed_surface")]
    surface: String,
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
    surface_url_lock(BrowserSurface::Embed)
        .lock()
        .expect("last embed active url")
        .clone()
}

pub fn human_browser_popout_active_url() -> String {
    surface_url_lock(BrowserSurface::Popout)
        .lock()
        .expect("last popout active url")
        .clone()
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
    active_tab_webview(app, BrowserSurface::Embed)
        .or_else(|| app.get_webview(EMBED_CONTENT_LABEL))
}

pub fn on_browser_popout_opened(app: &AppHandle) -> Result<(), String> {
    ensure_popout_shell(app)?;
    apply_popout_layout(app)?;
    if let Some(content) = popout_content_webview(app) {
        content.show().map_err(|err| err.to_string())?;
    }
    finalize_popout_compositing(app);
    Ok(())
}

pub fn on_browser_popout_closed(app: &AppHandle) -> Result<(), String> {
    if let Some(content) = popout_content_webview(app) {
        let _ = content.hide();
    }
    Ok(())
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

fn emit_loading(app: &AppHandle, surface: BrowserSurface, loading: bool) {
    let _ = app.emit(
        "human-browser-loading",
        HumanBrowserLoadingPayload {
            loading,
            surface: surface.as_str().to_string(),
        },
    );
}

fn emit_nav_state(app: &AppHandle, surface: BrowserSurface, can_go_back: bool, can_go_forward: bool) {
    let _ = app.emit(
        "human-browser-nav-state",
        HumanBrowserNavStatePayload {
            can_go_back,
            can_go_forward,
            surface: surface.as_str().to_string(),
        },
    );
}

fn emit_navigated(
    app: &AppHandle,
    surface: BrowserSurface,
    url: &str,
    title: Option<String>,
    favicon: Option<String>,
    tab_id: Option<String>,
) {
    let resolved_tab_id = tab_id.or_else(|| active_tab_id(surface));
    if resolved_tab_id.as_deref() == active_tab_id(surface).as_deref() {
        let mut guard = surface_url_lock(surface)
            .lock()
            .expect("surface active url");
        *guard = url.to_string();
    }
    let payload = HumanBrowserNavigatedPayload {
        url: url.to_string(),
        title,
        favicon,
        tab_id: resolved_tab_id,
        surface: surface.as_str().to_string(),
    };
    let _ = app.emit("human-browser-navigated", payload);
}

fn emit_new_window(app: &AppHandle, surface: BrowserSurface, url: &str) {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        return;
    }
    let _ = app.emit(
        "human-browser-new-window",
        HumanBrowserNewWindowPayload {
            url: trimmed.to_string(),
            surface: surface.as_str().to_string(),
        },
    );
}

fn parse_surface(raw: Option<&str>) -> BrowserSurface {
    match raw {
        Some("popout") => BrowserSurface::Popout,
        _ => BrowserSurface::Embed,
    }
}

fn probe_page_metadata(
    webview: &tauri::Webview,
    app: &AppHandle,
    url: &str,
    surface: BrowserSurface,
) {
    let url_json = serde_json::to_string(url).unwrap_or_else(|_| "\"\"".to_string());
    let surface_json =
        serde_json::to_string(surface.as_str()).unwrap_or_else(|_| "\"embed\"".to_string());
    let script = [
        "(function(){try{var u=",
        &url_json,
        ";var s=",
        &surface_json,
        ";var t=(document.title||'').trim();var fav=null;try{var link=document.querySelector('link[rel~=\"icon\"]');if(link&&link.href)fav=link.href;}catch(e){}var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;",
        "if(t)i.invoke('human_browser_report_title',{url:u,title:t,surface:s});",
        "if(fav)i.invoke('human_browser_report_favicon',{url:u,favicon:fav,surface:s});",
        "i.invoke('human_browser_report_nav_state',{canGoBack:window.history.length>1,canGoForward:false,surface:s});",
        "}catch(e){}})();",
    ]
    .concat();
    let _ = webview.eval(&script);
    let _ = app;
}

/// Intercept window.open and target=_blank clicks — WKWebView on_new_window misses many cases.
fn desktop_new_window_install_js(surface: BrowserSurface) -> String {
    let surface = surface.as_str();
    format!(
        r#"(function(){{if(window.__medousaNewWindowInstalled)return;window.__medousaNewWindowInstalled=true;function report(u){{if(!u||u==='about:blank')return;var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;i.invoke('human_browser_report_new_window',{{url:u,surface:'{surface}'}});}}var o=window.open;window.open=function(u){{report(u);return null}};document.addEventListener('click',function(e){{var a=e.target.closest&&e.target.closest('a[target="_blank"]');if(a&&a.href){{e.preventDefault();report(a.href)}}}},true);}})();"#
    )
}

fn content_builder(
    app: &AppHandle,
    label: String,
    tab_id: Option<String>,
    mobile_ua: bool,
    surface: BrowserSurface,
) -> WebviewBuilder<tauri::Wry> {
    let app_nav = app.clone();
    let app_load = app.clone();
    let app_new_window = app.clone();
    let surface_nav = surface;
    let surface_load = surface;
    let surface_new_window = surface;
    let tab_id_load = tab_id;
    let new_window_js = desktop_new_window_install_js(surface);
    let mut builder = WebviewBuilder::new(label, WebviewUrl::External("about:blank".parse().unwrap()))
        .on_new_window(move |url, _features| {
            let href = url.as_str();
            if !(href.starts_with("http://") || href.starts_with("https://")) {
                return NewWindowResponse::Deny;
            }
            if href == "about:blank" || href.contains("doubleclick") || href.contains("googlesyndication") {
                return NewWindowResponse::Deny;
            }
            emit_new_window(&app_new_window, surface_new_window, href);
            NewWindowResponse::Deny
        })
        .on_navigation(move |nav_url| {
            let href = nav_url.as_str();
            if href.starts_with("http://") || href.starts_with("https://") {
                emit_loading(&app_nav, surface_nav, true);
            }
            true
        })
        .on_page_load(move |webview, payload| {
            use tauri::webview::PageLoadEvent;
            match payload.event() {
                PageLoadEvent::Started => emit_loading(&app_load, surface_load, true),
                PageLoadEvent::Finished => {
                    emit_loading(&app_load, surface_load, false);
                    let href = payload.url().as_str().to_string();
                    emit_navigated(
                        &app_load,
                        surface_load,
                        &href,
                        None,
                        None,
                        tab_id_load.clone(),
                    );
                    probe_page_metadata(&webview, &app_load, &href, surface_load);
                    if mobile_ua {
                        let _ = webview.eval(MOBILE_EMBED_FIX_JS);
                        let _ = webview.eval(&new_window_js);
                    } else {
                        let _ = webview.eval(DESKTOP_EMBED_FILL_JS);
                        let _ = webview.eval(&new_window_js);
                    }
                }
            }
        });
    if mobile_ua {
        builder = builder.user_agent(MOBILE_SAFARI_UA);
    } else {
        builder = builder.background_color(EMBED_SURFACE_COLOR);
    }
    builder
}

fn chrome_builder(label: &'static str) -> WebviewBuilder<tauri::Wry> {
    WebviewBuilder::new(
        label,
        WebviewUrl::App("/popout/browser-chrome".into()),
    )
}

fn default_embed_layout() -> EmbedLayoutParams {
    EmbedLayoutParams {
        activity_width: 288.0,
        activity_collapsed: false,
        work_rail_visible: false,
        content_top: None,
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
    let chrome_top = params
        .content_top
        .filter(|value| *value > 0.0)
        .unwrap_or(CHROME_HEIGHT_LOGICAL);

    Ok(EmbedBounds {
        x: NAV_RAIL_WIDTH,
        y: chrome_top,
        width: (win_w - NAV_RAIL_WIDTH - activity_w).max(8.0),
        height: (win_h - chrome_top - bottom_chrome).max(8.0),
    })
}

fn default_mobile_embed_layout() -> EmbedMobileLayoutParams {
    EmbedMobileLayoutParams {
        bottom_chrome_height: MOBILE_BOTTOM_CHROME_DEFAULT,
        content_bounds: None,
    }
}

/// Drop embedded tab webviews so they can be recreated (e.g. when switching mobile/desktop UA).
fn reset_embedded_content(app: &AppHandle) -> Result<(), String> {
    close_legacy_content_webview(app, EMBED_CONTENT_LABEL);
    close_all_tab_webviews(app, BrowserSurface::Embed);
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

/// DOM `getBoundingClientRect` is in the **shell webview layout viewport** (top-left),
/// which on macOS begins below the title bar (`NSWindow.contentLayoutRect`, ~32px).
/// Tauri child `set_bounds` uses the **window contentView** (top-left, y=0 at view top).
fn dom_bounds_to_window_child_bounds(app: &AppHandle, dom: EmbedBounds) -> Result<EmbedBounds, String> {
    let (shell_x, shell_y) = shell_webview_origin(app)?;
    let (viewport_inset_x, viewport_inset_y) =
        macos_shell_viewport_origin_in_window(app).unwrap_or((0.0, 0.0));
    Ok(EmbedBounds {
        x: dom.x + shell_x + viewport_inset_x,
        y: dom.y + shell_y + viewport_inset_y,
        width: dom.width,
        height: dom.height,
    })
}

fn window_child_bounds_to_dom_bounds(app: &AppHandle, window_bounds: EmbedBounds) -> EmbedBounds {
    let (shell_x, shell_y) = shell_webview_origin(app).unwrap_or((0.0, 0.0));
    let (viewport_inset_x, viewport_inset_y) =
        macos_shell_viewport_origin_in_window(app).unwrap_or((0.0, 0.0));
    EmbedBounds {
        x: window_bounds.x - shell_x - viewport_inset_x,
        y: window_bounds.y - shell_y - viewport_inset_y,
        width: window_bounds.width,
        height: window_bounds.height,
    }
}

fn remember_viewport_inset(inset: (f64, f64)) {
    if let Ok(mut last) = LAST_VIEWPORT_INSET.lock() {
        *last = Some(inset);
    }
}

fn last_viewport_inset() -> Option<(f64, f64)> {
    LAST_VIEWPORT_INSET.lock().ok().and_then(|guard| *guard)
}

/// Where the shell JS layout viewport origin sits inside the window contentView (top-left).
/// On macOS this is the title-bar / toolbar inset (`contentLayoutRect.origin` in top-left terms).
#[cfg(target_os = "macos")]
fn macos_shell_viewport_origin_in_window(app: &AppHandle) -> Option<(f64, f64)> {
    use std::sync::{Arc, Mutex};
    let shell = app.get_webview(MAIN_WINDOW_LABEL)?;
    let out = Arc::new(Mutex::new(None::<(f64, f64)>));
    let capture = Arc::clone(&out);
    let _ = shell.with_webview(move |w| unsafe {
        use objc2_app_kit::{NSView, NSWindow};
        let view: &NSView = &*w.inner().cast();
        let Some(window) = view.window() else {
            return;
        };
        let Some(content_view) = window.contentView() else {
            return;
        };
        let layout = window.contentLayoutRect();
        let x_inset = layout.origin.x - content_view.bounds().origin.x;
        let y_inset = macos_rect_top_left_in_view(layout, &*content_view);
        if let Ok(mut slot) = capture.lock() {
            *slot = Some((x_inset.max(0.0), y_inset.max(0.0)));
        }
    });
    // Prefer reading the mutex over `try_unwrap` — with_webview may still hold the Arc briefly.
    let measured = out.lock().ok().and_then(|guard| *guard);
    if let Some(inset) = measured {
        remember_viewport_inset(inset);
        return Some(inset);
    }
    last_viewport_inset()
}

#[cfg(not(target_os = "macos"))]
fn macos_shell_viewport_origin_in_window(_app: &AppHandle) -> Option<(f64, f64)> {
    None
}

fn webview_tauri_bounds_logical(
    app: &AppHandle,
    label: &str,
) -> Result<(f64, f64, f64, f64), String> {
    let window = workshop_window(app)?;
    let scale = window.scale_factor().map_err(|err| err.to_string())?;
    let webview = app
        .get_webview(label)
        .ok_or_else(|| format!("webview {label} missing"))?;
    let rect = webview.bounds().map_err(|err| err.to_string())?;
    let pos = rect.position.to_logical::<f64>(scale);
    let size = rect.size.to_logical::<f64>(scale);
    Ok((pos.x, pos.y, size.width, size.height))
}

/// AppKit `CGRect.origin` is bottom-left on non-flipped views; Tauri/wry use top-left.
#[cfg(target_os = "macos")]
unsafe fn macos_rect_top_left_in_view(rect: objc2_foundation::NSRect, view: &objc2_app_kit::NSView) -> f64 {
    if view.isFlipped() {
        rect.origin.y
    } else {
        view.bounds().size.height - rect.origin.y - rect.size.height
    }
}

#[cfg(target_os = "macos")]
fn macos_webview_frame_in_window_content(
    webview: &tauri::Webview,
) -> Option<(f64, f64, f64, f64)> {
    use std::sync::{Arc, Mutex};
    let out = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&out);
    let _ = webview.with_webview(move |w| {
        unsafe {
            use objc2_app_kit::{NSView, NSWindow};
            let view: &NSView = &*w.inner().cast();
            let bounds = view.bounds();
            let Some(window) = view.window() else {
                return;
            };
            let Some(content_view) = window.contentView() else {
                return;
            };
            let converted = view.convertRect_toView(bounds, Some(&*content_view));
            let y_top = macos_rect_top_left_in_view(converted, &*content_view);
            if let Ok(mut guard) = capture.lock() {
                *guard = Some((
                    converted.origin.x,
                    y_top,
                    converted.size.width,
                    converted.size.height,
                ));
            }
        }
    });
    Arc::try_unwrap(out).ok()?.into_inner().ok().flatten()
}

/// wry `set_bounds` checks `isFlipped()` but `bounds()` readback always assumes non-flipped.
#[cfg(target_os = "macos")]
fn macos_webview_layout_diagnostics(webview: &tauri::Webview) -> Option<serde_json::Value> {
    use std::sync::{Arc, Mutex};
    let out = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&out);
    let _ = webview.with_webview(move |w| {
        unsafe {
            use objc2_app_kit::NSView;
            let view: &NSView = &*w.inner().cast();
            let superview = view.superview();
            let super_flipped = superview.as_ref().map(|v| v.isFlipped());
            let content_flipped = view
                .window()
                .and_then(|window| window.contentView())
                .map(|cv| cv.isFlipped());
            if let Ok(mut guard) = capture.lock() {
                *guard = Some(serde_json::json!({
                    "superviewIsFlipped": super_flipped,
                    "contentViewIsFlipped": content_flipped,
                }));
            }
        }
    });
    Arc::try_unwrap(out).ok()?.into_inner().ok().flatten()
}

fn reset_js_scroll(webview: &tauri::Webview) {
    let _ = webview.eval(
        r#"(function(){try{window.scrollTo(0,0);if(document.documentElement)document.documentElement.scrollTop=0;if(document.body)document.body.scrollTop=0;}catch(e){}})();"#,
    );
}

/// Desktop main embed z-order (mirrors pop-out): content above shell, chrome above content.
#[cfg(target_os = "macos")]
fn macos_order_webview_above(app: &AppHandle, label: &str, relative_to: &str) -> bool {
    use std::sync::{Arc, Mutex};
    let Some(webview) = app.get_webview(label) else {
        return false;
    };
    let Some(relative) = app.get_webview(relative_to) else {
        return false;
    };
    let rel_ptr = Arc::new(Mutex::new(0usize));
    let rel_capture = Arc::clone(&rel_ptr);
    let _ = relative.with_webview(move |w| {
        if let Ok(mut slot) = rel_capture.lock() {
            *slot = w.inner() as usize;
        }
    });
    let rel_addr = rel_ptr.lock().map(|g| *g).unwrap_or(0);
    if rel_addr == 0 {
        return false;
    }
    let ordered = Arc::new(Mutex::new(false));
    let ordered_capture = Arc::clone(&ordered);
    let _ = webview.with_webview(move |w| unsafe {
        use objc2_app_kit::{NSView, NSWindowOrderingMode};
        let view: &NSView = &*w.inner().cast();
        let rel_view: &NSView = &*(rel_addr as *const std::ffi::c_void).cast::<NSView>();
        if let Some(parent) = view.superview() {
            parent.addSubview_positioned_relativeTo(
                view,
                NSWindowOrderingMode::Above,
                Some(rel_view),
            );
            if let Ok(mut slot) = ordered_capture.lock() {
                *slot = true;
            }
        }
    });
    ordered.lock().map(|g| *g).unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn macos_ensure_content_webview_opaque(app: &AppHandle) {
    let Some(content) = embedded_content_webview(app) else {
        return;
    };
    let _ = content.with_webview(|w| unsafe {
        use objc2_app_kit::NSView;
        let view: &NSView = &*w.inner().cast();
        if !view.isOpaque() {
            let _: () = objc2::msg_send![view, setOpaque: true];
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn macos_ensure_content_webview_opaque(_app: &AppHandle) {}

#[cfg(target_os = "macos")]
fn macos_sync_content_webview_clip(app: &AppHandle, _target: EmbedBounds) {
    let Some(content) = embedded_content_webview(app) else {
        return;
    };
    let _ = content.with_webview(|w| unsafe {
        use objc2_app_kit::NSView;
        let view: &NSView = &*w.inner().cast();
        view.setClipsToBounds(true);
        view.layoutSubtreeIfNeeded();
    });
}

#[cfg(not(target_os = "macos"))]
fn macos_sync_content_webview_clip(_app: &AppHandle, _target: EmbedBounds) {}

#[cfg(target_os = "macos")]
fn macos_ensure_desktop_embed_z_order(app: &AppHandle) {
    if MOBILE_SHELL_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    let Some(tab_id) = active_tab_id(BrowserSurface::Embed) else {
        return;
    };
    let label = tab_webview_label(BrowserSurface::Embed, &tab_id);
    macos_order_webview_above(app, &label, MAIN_WINDOW_LABEL);
}

#[cfg(not(target_os = "macos"))]
fn macos_ensure_desktop_embed_z_order(_app: &AppHandle) {}

/// Pop-out: tab content above the shell webview, chrome strip above content.
#[cfg(target_os = "macos")]
fn macos_ensure_popout_z_order(app: &AppHandle) {
    let Some(tab_id) = active_tab_id(BrowserSurface::Popout) else {
        return;
    };
    let tab_label = tab_webview_label(BrowserSurface::Popout, &tab_id);
    macos_order_webview_above(app, &tab_label, BROWSER_WINDOW_LABEL);
    macos_order_webview_above(app, BROWSER_CHROME_LABEL, &tab_label);
}

#[cfg(not(target_os = "macos"))]
fn macos_ensure_popout_z_order(_app: &AppHandle) {}

fn finalize_popout_compositing(app: &AppHandle) {
    if let Some(main) = popout_main_webview(app) {
        let _ = main.set_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(1.0, 1.0).into(),
        });
        let _ = main.hide();
    }
    macos_ensure_popout_z_order(app);
    if let Some(chrome) = popout_chrome_webview(app) {
        let _ = chrome.show();
    }
}

fn finalize_desktop_embed_compositing(app: &AppHandle, target: Option<EmbedBounds>) {
    if MOBILE_SHELL_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    macos_ensure_desktop_embed_z_order(app);
    if let Some(bounds) = target {
        macos_sync_content_webview_clip(app, bounds);
    }
    macos_ensure_content_webview_opaque(app);
}

/// Which sibling is painted last (higher index = on top in AppKit).
#[cfg(target_os = "macos")]
fn macos_subview_stack_probe(app: &AppHandle) -> Option<serde_json::Value> {
    use std::sync::{Arc, Mutex};
    let content = embedded_content_webview(app)?;
    let shell = app.get_webview(MAIN_WINDOW_LABEL)?;
    let shell_ptr = Arc::new(Mutex::new(0usize));
    let shell_capture = Arc::clone(&shell_ptr);
    let _ = shell.with_webview(move |shell_w| {
        if let Ok(mut slot) = shell_capture.lock() {
            *slot = shell_w.inner() as usize;
        }
    });
    let shell_addr = shell_ptr.lock().map(|g| *g).unwrap_or(0);
    if shell_addr == 0 {
        return None;
    }
    let result = Arc::new(Mutex::new(None));
    let result_capture = Arc::clone(&result);
    let _ = content.with_webview(move |content_w| unsafe {
        use objc2_app_kit::NSView;
        let content_view: &NSView = &*content_w.inner().cast();
        let shell_view: &NSView = &*(shell_addr as *const std::ffi::c_void).cast::<NSView>();
        let Some(parent) = content_view.superview() else {
            return;
        };
        let subs = parent.subviews();
        let mut shell_idx = None;
        let mut content_idx = None;
        for (i, sub) in subs.iter().enumerate() {
            if std::ptr::eq(&*sub, shell_view) {
                shell_idx = Some(i);
            }
            if std::ptr::eq(&*sub, content_view) {
                content_idx = Some(i);
            }
        }
        if let Ok(mut slot) = result_capture.lock() {
            *slot = Some(serde_json::json!({
                "subviewCount": subs.len(),
                "shellIndex": shell_idx,
                "contentIndex": content_idx,
                "contentAboveShell": match (shell_idx, content_idx) {
                    (Some(s), Some(c)) => Some(c > s),
                    _ => None,
                },
                "shellAboveContent": match (shell_idx, content_idx) {
                    (Some(s), Some(c)) => Some(s > c),
                    _ => None,
                },
            }));
        }
    });
    result.lock().ok().and_then(|guard| guard.clone())
}

#[cfg(not(target_os = "macos"))]
fn macos_subview_stack_probe(_app: &AppHandle) -> Option<serde_json::Value> {
    None
}

#[cfg(not(target_os = "macos"))]
fn macos_webview_layout_diagnostics(_webview: &tauri::Webview) -> Option<serde_json::Value> {
    None
}

#[cfg(not(target_os = "macos"))]
fn macos_webview_frame_in_window_content(_webview: &tauri::Webview) -> Option<(f64, f64, f64, f64)> {
    None
}

fn bounds_json(x: f64, y: f64, w: f64, h: f64) -> serde_json::Value {
    serde_json::json!({ "x": x, "y": y, "width": w, "height": h, "bottom": y + h, "right": x + w })
}

fn coordinate_frame_snapshot(app: &AppHandle, dom: Option<EmbedBounds>) -> Result<serde_json::Value, String> {
    let window = workshop_window(app)?;
    let (win_w, win_h) = window_inner_logical(&window)?;
    let scale = window.scale_factor().map_err(|err| err.to_string())?;
    let (shell_x, shell_y) = shell_webview_origin(app)?;
    let (main_x, main_y, main_w, main_h) = webview_tauri_bounds_logical(app, MAIN_WINDOW_LABEL)?;
    let workshop = compute_embedded_bounds(&window, default_embed_layout())?;

    let content_tauri = embedded_content_webview(app).map(|content| {
        let rect = content.bounds().ok();
        rect.map(|rect| {
            let pos = rect.position.to_logical::<f64>(scale);
            let size = rect.size.to_logical::<f64>(scale);
            bounds_json(pos.x, pos.y, size.width, size.height)
        })
    }).flatten();

    let main_native = app
        .get_webview(MAIN_WINDOW_LABEL)
        .and_then(|wv| macos_webview_frame_in_window_content(&wv))
        .map(|(x, y, w, h)| bounds_json(x, y, w, h));

    let content_native = embedded_content_webview(app)
        .and_then(|wv| macos_webview_frame_in_window_content(&wv))
        .map(|(x, y, w, h)| bounds_json(x, y, w, h));

    let dom_target = dom.map(|d| dom_bounds_to_window_child_bounds(app, d).ok()).flatten();

    let dom_vs_content_native = match (dom_target, content_native.as_ref()) {
        (Some(target), Some(native)) => Some(serde_json::json!({
            "x": target.x - native["x"].as_f64().unwrap_or(0.0),
            "y": target.y - native["y"].as_f64().unwrap_or(0.0),
            "w": target.width - native["width"].as_f64().unwrap_or(0.0),
            "h": target.height - native["height"].as_f64().unwrap_or(0.0),
        })),
        _ => None,
    };

    let macos_diagnostics = embedded_content_webview(app)
        .and_then(|wv| macos_webview_layout_diagnostics(&wv));

    Ok(serde_json::json!({
        "frames": {
            "windowInner": { "width": win_w, "height": win_h },
            "shellTauriOrigin": { "x": shell_x, "y": shell_y },
            "mainShellTauri": bounds_json(main_x, main_y, main_w, main_h),
            "mainShellNativeInWindow": main_native,
            "contentTauri": content_tauri,
            "contentNativeInWindow": content_native,
            "workshopLayout": bounds_json(workshop.x, workshop.y, workshop.width, workshop.height),
            "domViewport": dom.map(|d| bounds_json(d.x, d.y, d.width, d.height)),
            "domToWindowTarget": dom_target.map(|d| bounds_json(d.x, d.y, d.width, d.height)),
        },
        "deltas": {
            "workshopYMinusDomY": dom.map(|d| workshop.y - d.y),
            "workshopBottomMinusDomBottom": dom.map(|d| (workshop.y + workshop.height) - (d.y + d.height)),
            "domVsContentNative": dom_vs_content_native,
            "shellOriginY": shell_y,
        },
        "macosDiagnostics": macos_diagnostics,
        "subviewStack": macos_subview_stack_probe(app),
        "note": "domViewport = getBoundingClientRect (top-left). contentNativeInWindow = AppKit top-left in window contentView. workshopLayout = stale Rust math.",
    }))
}

fn shell_webview_origin(app: &AppHandle) -> Result<(f64, f64), String> {
    let window = workshop_window(app)?;
    let scale = window.scale_factor().map_err(|err| err.to_string())?;
    let Some(shell) = app.get_webview(MAIN_WINDOW_LABEL) else {
        return Ok((0.0, 0.0));
    };
    let origin = shell
        .bounds()
        .map_err(|err| err.to_string())?
        .position
        .to_logical::<f64>(scale);
    Ok((origin.x, origin.y))
}

fn embed_freeform_dom_bounds(app: &AppHandle) -> Option<EmbedBounds> {
    LAST_EMBED_PLACEMENT
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .and_then(|placement| match placement {
            EmbedPlacement::Freeform(bounds) => Some(bounds),
            _ => None,
        })
}

/// Swap which embed tab webview is visible — does not change bounds (avoids gap-correction drift).
fn show_active_embed_tab(app: &AppHandle) -> Result<(), String> {
    let active = active_tab_id(BrowserSurface::Embed);
    let tab_ids = tab_ids_lock(BrowserSurface::Embed)
        .lock()
        .ok()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let embed_visible = EMBED_VISIBLE.load(Ordering::SeqCst);

    for tab_id in tab_ids {
        let Some(content) = tab_webview(app, BrowserSurface::Embed, &tab_id) else {
            continue;
        };
        if embed_visible && active.as_deref() == Some(tab_id.as_str()) {
            content.show().map_err(|err| err.to_string())?;
        } else {
            let _ = content.hide();
        }
    }

    if embed_visible {
        if let Some(dom) = embed_freeform_dom_bounds(app) {
            if let Ok(target) = dom_bounds_to_window_child_bounds(app, dom) {
                finalize_desktop_embed_compositing(app, Some(target));
            }
        }
    }
    Ok(())
}

fn apply_embedded_bounds(app: &AppHandle, bounds: EmbedBounds) -> Result<(), String> {
    let width = bounds.width.max(8.0);
    let height = bounds.height.max(8.0);
    let rect = Rect {
        position: LogicalPosition::new(bounds.x, bounds.y).into(),
        size: LogicalSize::new(width, height).into(),
    };
    if let Some(shell) = app.get_webview(MAIN_WINDOW_LABEL) {
        reset_js_scroll(&shell);
    }

    let active = active_tab_id(BrowserSurface::Embed);
    let tab_ids = tab_ids_lock(BrowserSurface::Embed)
        .lock()
        .ok()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    let embed_visible = EMBED_VISIBLE.load(Ordering::SeqCst);

    if !tab_ids.is_empty() {
        for tab_id in tab_ids {
            let Some(content) = tab_webview(app, BrowserSurface::Embed, &tab_id) else {
                continue;
            };
            content
                .set_bounds(rect)
                .map_err(|err| err.to_string())?;
            if embed_visible && active.as_deref() == Some(tab_id.as_str()) {
                content.show().map_err(|err| err.to_string())?;
            } else {
                let _ = content.hide();
            }
        }
        return Ok(());
    }

    if let Some(content) = embedded_content_webview(app) {
        content.set_bounds(rect).map_err(|err| err.to_string())?;
        if embed_visible {
            content.show().map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn apply_embedded_dom_bounds(app: &AppHandle, dom: EmbedBounds) -> Result<(), String> {
    let target = dom_bounds_to_window_child_bounds(app, dom)?;
    apply_embedded_bounds(app, target)?;

    for _ in 0..2 {
        let Some(content) = embedded_content_webview(app) else {
            return Ok(());
        };

        #[cfg(target_os = "macos")]
        let (actual_x, actual_y, actual_w, actual_h) =
            if let Some((x, y, w, h)) = macos_webview_frame_in_window_content(&content) {
                (x, y, w, h)
            } else {
                let window = workshop_window(app)?;
                let scale = window.scale_factor().map_err(|err| err.to_string())?;
                let rect = content.bounds().map_err(|err| err.to_string())?;
                let pos = rect.position.to_logical::<f64>(scale);
                let size = rect.size.to_logical::<f64>(scale);
                (pos.x, pos.y, size.width, size.height)
            };

        #[cfg(not(target_os = "macos"))]
        let (actual_x, actual_y, actual_w, actual_h) = {
            let window = workshop_window(app)?;
            let scale = window.scale_factor().map_err(|err| err.to_string())?;
            let rect = content.bounds().map_err(|err| err.to_string())?;
            let pos = rect.position.to_logical::<f64>(scale);
            let size = rect.size.to_logical::<f64>(scale);
            (pos.x, pos.y, size.width, size.height)
        };

        let gap_x = target.x - actual_x;
        let gap_y = target.y - actual_y;
        let gap_w = target.width - actual_w;
        let gap_h = target.height - actual_h;

        if gap_x.abs() <= 2.0
            && gap_y.abs() <= 2.0
            && gap_w.abs() <= 2.0
            && gap_h.abs() <= 2.0
        {
            break;
        }

        apply_embedded_bounds(
            app,
            EmbedBounds {
                x: target.x + gap_x,
                y: target.y + gap_y,
                width: target.width + gap_w,
                height: target.height + gap_h,
            },
        )?;
    }
    finalize_desktop_embed_compositing(app, Some(target));
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
    finalize_desktop_embed_compositing(app, Some(bounds));
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
    if !MOBILE_SHELL_ACTIVE.load(Ordering::SeqCst) {
        ensure_embedded_desktop_profile(app)?;
    }
    if let Ok(mut last) = LAST_EMBED_PLACEMENT.lock() {
        *last = Some(EmbedPlacement::Freeform(bounds));
    }
    apply_embedded_dom_bounds(app, bounds)
}

fn reapply_embedded_placement(app: &AppHandle) -> Result<(), String> {
    let Some(placement) = LAST_EMBED_PLACEMENT.lock().ok().and_then(|guard| *guard) else {
        return Ok(());
    };
    match placement {
        EmbedPlacement::Workshop(params) => apply_embedded_layout(app, params),
        EmbedPlacement::Mobile(params) => apply_embedded_mobile_layout(app, params).map(|_| ()),
        EmbedPlacement::Freeform(bounds) => apply_embedded_dom_bounds(app, bounds),
    }
}

fn current_embed_bounds(app: &AppHandle) -> Result<EmbedBounds, String> {
    let window = workshop_window(app)?;
    match LAST_EMBED_PLACEMENT.lock().ok().and_then(|guard| *guard) {
        Some(EmbedPlacement::Freeform(bounds)) => Ok(bounds),
        Some(EmbedPlacement::Workshop(params)) => compute_embedded_bounds(&window, params),
        Some(EmbedPlacement::Mobile(params)) => compute_mobile_embedded_bounds(&window, params),
        None => Ok(EmbedBounds {
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 8.0,
        }),
    }
}

/// DOM-measured embed bounds (compositor / mobile) → window contentView coords (title-bar inset).
fn embed_stored_bounds_to_window(app: &AppHandle, bounds: EmbedBounds) -> Result<EmbedBounds, String> {
    let needs_dom_convert = LAST_EMBED_PLACEMENT
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .map(|placement| match placement {
            EmbedPlacement::Freeform(_) => true,
            EmbedPlacement::Mobile(params) => params.content_bounds.is_some(),
            EmbedPlacement::Workshop(_) => false,
        })
        .unwrap_or(false);
    if needs_dom_convert {
        dom_bounds_to_window_child_bounds(app, bounds)
    } else {
        Ok(bounds)
    }
}

fn current_embed_window_bounds(app: &AppHandle) -> Result<EmbedBounds, String> {
    embed_stored_bounds_to_window(app, current_embed_bounds(app)?)
}

fn current_popout_content_bounds(app: &AppHandle) -> Result<EmbedBounds, String> {
    let (x, y, width, height) = current_popout_content_rect(app)?;
    Ok(EmbedBounds {
        x,
        y,
        width,
        height,
    })
}

fn current_popout_content_rect(app: &AppHandle) -> Result<(f64, f64, f64, f64), String> {
    let window = popout_window(app)?;
    let (width, height) = window_inner_logical(&window)?;
    let content_height = (height - POPOUT_CHROME_HEIGHT_LOGICAL).max(8.0);
    Ok((0.0, POPOUT_CHROME_HEIGHT_LOGICAL, width, content_height))
}

fn create_tab_webview(
    app: &AppHandle,
    tab_id: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    if tab_webview(app, BrowserSurface::Embed, tab_id).is_some() {
        return Ok(());
    }
    let label = tab_webview_label(BrowserSurface::Embed, tab_id);
    let window = workshop_window(app)?;
    let mobile_ua = EMBED_MOBILE_UA.load(Ordering::SeqCst);
    window
        .add_child(
            content_builder(
                app,
                label,
                Some(tab_id.to_string()),
                mobile_ua,
                BrowserSurface::Embed,
            ),
            LogicalPosition::new(x, y),
            LogicalSize::new(width.max(8.0), height.max(8.0)),
        )
        .map_err(|err| err.to_string())?;
    register_tab_id(BrowserSurface::Embed, tab_id);
    EMBED_READY.store(true, Ordering::SeqCst);
    Ok(())
}

fn apply_tab_webview_bounds(
    app: &AppHandle,
    surface: BrowserSurface,
    tab_id: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let Some(webview) = tab_webview(app, surface, tab_id) else {
        return Ok(());
    };
    webview
        .set_bounds(Rect {
            position: LogicalPosition::new(x, y).into(),
            size: LogicalSize::new(width.max(8.0), height.max(8.0)).into(),
        })
        .map_err(|err| err.to_string())
}

fn navigate_tab_webview(
    app: &AppHandle,
    surface: BrowserSurface,
    url: &str,
    force: bool,
) -> Result<(), String> {
    let trimmed = url.trim();
    let content = match surface {
        BrowserSurface::Embed => embedded_content_webview(app),
        BrowserSurface::Popout => popout_content_webview(app),
    }
    .ok_or_else(|| "browser content not ready".to_string())?;

    if trimmed.is_empty() || trimmed == "about:blank" {
        content
            .navigate(
                "about:blank"
                    .parse()
                    .map_err(|err: url::ParseError| err.to_string())?,
            )
            .map_err(|err| err.to_string())?;
        emit_navigated(app, surface, "about:blank", None, None, None);
        return Ok(());
    }

    if !force {
        if let Ok(current) = content.url() {
            if urls_match_for_snapshot(current.as_ref(), trimmed) {
                emit_loading(app, surface, false);
                return Ok(());
            }
        }
    }

    let external = parse_external_url(trimmed)?;
    content.navigate(external).map_err(|err| err.to_string())?;
    emit_loading(app, surface, true);
    emit_navigated(app, surface, trimmed, None, None, None);
    Ok(())
}

fn is_blank_browser_url(url: &str) -> bool {
    let trimmed = url.trim();
    trimmed.is_empty() || trimmed == "about:blank"
}

fn hide_embed_surface(app: &AppHandle) {
    EMBED_VISIBLE.store(false, Ordering::SeqCst);
    let ids = tab_ids_lock(BrowserSurface::Embed)
        .lock()
        .ok()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    for id in ids {
        if let Some(content) = tab_webview(app, BrowserSurface::Embed, &id) {
            let _ = content.hide();
        }
    }
    if let Some(content) = app.get_webview(EMBED_CONTENT_LABEL) {
        let _ = content.hide();
    }
}

fn activate_embed_tab(
    app: &AppHandle,
    tab_id: &str,
    initial_url: &str,
) -> Result<(), String> {
    close_legacy_content_webview(app, EMBED_CONTENT_LABEL);

    {
        let mut guard = active_tab_id_lock(BrowserSurface::Embed)
            .lock()
            .map_err(|_| "active tab lock poisoned".to_string())?;
        *guard = Some(tab_id.to_string());
    }
    register_tab_id(BrowserSurface::Embed, tab_id);
    hide_tab_webviews(app, BrowserSurface::Embed, Some(tab_id));

    let blank = is_blank_browser_url(initial_url);
    let exists = tab_webview(app, BrowserSurface::Embed, tab_id).is_some();
    if !exists {
        let (x, y, w, h) = if let Some(dom) = embed_freeform_dom_bounds(app) {
            let window_bounds = dom_bounds_to_window_child_bounds(app, dom)?;
            (
                window_bounds.x,
                window_bounds.y,
                window_bounds.width,
                window_bounds.height,
            )
        } else {
            (0.0, 0.0, 8.0, 8.0)
        };
        create_tab_webview(app, tab_id, x, y, w, h)?;
        if blank {
            // Hide before navigate — add_child can show immediately while
            // EMBED_VISIBLE is still true from the previous page tab.
            hide_embed_surface(app);
        }
        navigate_tab_webview(app, BrowserSurface::Embed, initial_url, true)?;
        if blank {
            // Start page owns the UI — keep the blank native webview hidden.
            hide_embed_surface(app);
            return Ok(());
        }
        if let Some(dom) = embed_freeform_dom_bounds(app) {
            let target = dom_bounds_to_window_child_bounds(app, dom)?;
            apply_embedded_bounds(app, target)?;
            finalize_desktop_embed_compositing(app, Some(target));
        }
    } else {
        navigate_tab_webview(app, BrowserSurface::Embed, initial_url, false)?;
        emit_loading(app, BrowserSurface::Embed, false);
        if blank {
            hide_embed_surface(app);
            return Ok(());
        }
        // Existing tab — only swap visibility; re-layout corrupts bounds via gap correction.
        show_active_embed_tab(app)?;
    }

    Ok(())
}

fn activate_popout_tab(
    app: &AppHandle,
    tab_id: &str,
    initial_url: &str,
) -> Result<(), String> {
    ensure_popout_shell(app)?;

    {
        let mut guard = active_tab_id_lock(BrowserSurface::Popout)
            .lock()
            .map_err(|_| "active tab lock poisoned".to_string())?;
        *guard = Some(tab_id.to_string());
    }

    apply_popout_layout(app)?;

    let popout_visible = app
        .get_webview_window(BROWSER_WINDOW_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);

    let content = popout_content_webview(app)
        .ok_or_else(|| "pop-out browser content not ready".to_string())?;

    if popout_visible {
        content.show().map_err(|err| err.to_string())?;
    }

    navigate_tab_webview(app, BrowserSurface::Popout, initial_url, true)?;
    finalize_popout_compositing(app);
    Ok(())
}

fn activate_surface_tab(
    app: &AppHandle,
    surface: BrowserSurface,
    tab_id: &str,
    initial_url: &str,
) -> Result<(), String> {
    match surface {
        BrowserSurface::Embed => activate_embed_tab(app, tab_id, initial_url),
        BrowserSurface::Popout => activate_popout_tab(app, tab_id, initial_url),
    }
}

fn close_surface_tab(app: &AppHandle, surface: BrowserSurface, tab_id: &str) -> Result<(), String> {
    match surface {
        BrowserSurface::Embed => {
            if let Some(webview) = tab_webview(app, BrowserSurface::Embed, tab_id) {
                webview.close().map_err(|err| err.to_string())?;
            }
            unregister_tab_id(BrowserSurface::Embed, tab_id);
        }
        BrowserSurface::Popout => {
            // Pop-out uses a single shared content webview — only drop tab metadata.
        }
    }
    if active_tab_id(surface).as_deref() == Some(tab_id) {
        if let Ok(mut guard) = active_tab_id_lock(surface).lock() {
            *guard = None;
        }
    }
    Ok(())
}

fn flush_pending_embed_navigation(app: &AppHandle) {
    let url = human_browser_active_url();
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        return;
    }
    let Some(tab_id) = active_tab_id(BrowserSurface::Embed) else {
        return;
    };
    if tab_webview(app, BrowserSurface::Embed, &tab_id).is_none() {
        let _ = activate_surface_tab(app, BrowserSurface::Embed, &tab_id, trimmed);
        return;
    }
    let _ = navigate_tab_webview(app, BrowserSurface::Embed, trimmed, false);
}

fn navigate_embedded_url(app: &AppHandle, url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    {
        let mut guard = surface_url_lock(BrowserSurface::Embed)
            .lock()
            .expect("embed active url");
        *guard = trimmed.to_string();
    }
    navigate_tab_webview(app, BrowserSurface::Embed, trimmed, true)
}

fn navigate_popout_url(app: &AppHandle, url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    {
        let mut guard = surface_url_lock(BrowserSurface::Popout)
            .lock()
            .expect("popout active url");
        *guard = trimmed.to_string();
    }
    ensure_popout_shell(app)?;
    navigate_tab_webview(app, BrowserSurface::Popout, trimmed, true)
}

fn create_embedded_content_at(app: &AppHandle, bounds: EmbedBounds) -> Result<(), String> {
    if embedded_content_webview(app).is_some() {
        return Ok(());
    }
    let Some(tab_id) = active_tab_id(BrowserSurface::Embed) else {
        return Ok(());
    };
    create_tab_webview(app, &tab_id, 0.0, 0.0, 8.0, 8.0)?;
    apply_embedded_dom_bounds(app, bounds)
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
        None if MOBILE_SHELL_ACTIVE.load(Ordering::SeqCst) => EmbedBounds {
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 8.0,
        },
        // Desktop compositor — defer until activate_tab supplies a tab webview.
        None => return Ok(()),
    };

    create_embedded_content_at(app, initial_bounds)?;

    if EMBED_VISIBLE.load(Ordering::SeqCst) {
        match LAST_EMBED_PLACEMENT.lock().ok().and_then(|guard| *guard) {
            Some(EmbedPlacement::Freeform(bounds)) => {
                apply_embedded_dom_bounds(app, bounds)?;
            }
            _ => apply_embedded_bounds(app, initial_bounds)?,
        }
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
    let placement = LAST_EMBED_PLACEMENT.lock().ok().and_then(|guard| *guard);
    match placement {
        // Compositor calls set_bounds immediately before show — reapply DOM bounds + show.
        Some(EmbedPlacement::Freeform(bounds)) => {
            apply_embedded_dom_bounds(&app, bounds)?;
        }
        Some(_) => {
            reapply_embedded_placement(&app)?;
        }
        None => {
            if let Some(content) = embedded_content_webview(&app) {
                content.show().map_err(|err| err.to_string())?;
            }
        }
    }
    flush_pending_embed_navigation(&app);
    Ok(())
}

pub fn on_main_window_resized(app: &AppHandle) {
    if !EMBED_VISIBLE.load(Ordering::SeqCst) {
        return;
    }
    // Mobile shell and DOM-measured freeform bounds are updated by the frontend compositor.
    if MOBILE_SHELL_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    if LAST_EMBED_PLACEMENT
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .is_some_and(|placement| matches!(placement, EmbedPlacement::Freeform(_)))
    {
        let _ = app.emit("human-browser-window-resized", ());
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
    /// Main shell webview origin in window space (debug — should be 0,0 on desktop).
    pub shell_origin_x: f64,
    pub shell_origin_y: f64,
}

#[tauri::command]
pub fn human_browser_embed_coord_probe(
    app: AppHandle,
    dom: Option<EmbedBoundsDto>,
) -> Result<serde_json::Value, String> {
    let dom_bounds = dom.map(EmbedBounds::from);
    coordinate_frame_snapshot(&app, dom_bounds)
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
    let (shell_origin_x, shell_origin_y) = shell_webview_origin(&app).unwrap_or((0.0, 0.0));
    let dom = window_child_bounds_to_dom_bounds(
        &app,
        EmbedBounds {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
        },
    );
    Ok(EmbedBoundsReadback {
        x: dom.x,
        y: dom.y,
        width: dom.width,
        height: dom.height,
        window_width: win_w,
        window_height: win_h,
        shell_origin_x,
        shell_origin_y,
    })
}

#[tauri::command]
pub fn human_browser_embed_hide(app: AppHandle) -> Result<(), String> {
    hide_embed_surface(&app);
    Ok(())
}

fn apply_popout_layout(app: &AppHandle) -> Result<(), String> {
    let window = popout_window(app)?;
    let (width, height) = window_inner_logical(&window)?;
    let content_height = (height - POPOUT_CHROME_HEIGHT_LOGICAL).max(8.0);

    if let Some(main) = popout_main_webview(app) {
        let _ = main.set_bounds(Rect {
            position: LogicalPosition::new(0.0, 0.0).into(),
            size: LogicalSize::new(1.0, 1.0).into(),
        });
        let _ = main.hide();
    }

    let popout_visible = app
        .get_webview_window(BROWSER_WINDOW_LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);

    if let Some(content) = popout_content_webview(app) {
        content
            .set_bounds(Rect {
                position: LogicalPosition::new(0.0, POPOUT_CHROME_HEIGHT_LOGICAL).into(),
                size: LogicalSize::new(width, content_height).into(),
            })
            .map_err(|err| err.to_string())?;
        if popout_visible {
            content.show().map_err(|err| err.to_string())?;
        }
    }

    if let Some(chrome) = popout_chrome_webview(app) {
        chrome
            .set_bounds(Rect {
                position: LogicalPosition::new(0.0, 0.0).into(),
                size: LogicalSize::new(width, POPOUT_CHROME_HEIGHT_LOGICAL).into(),
            })
            .map_err(|err| err.to_string())?;
        chrome.show().map_err(|err| err.to_string())?;
    }

    finalize_popout_compositing(app);
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
    let content_height = (height - POPOUT_CHROME_HEIGHT_LOGICAL).max(8.0);

    if popout_content_webview(app).is_none() {
        window
            .add_child(
                content_builder(
                    app,
                    BROWSER_CONTENT_LABEL.to_string(),
                    None,
                    false,
                    BrowserSurface::Popout,
                ),
                LogicalPosition::new(0.0, POPOUT_CHROME_HEIGHT_LOGICAL),
                LogicalSize::new(width, content_height),
            )
            .map_err(|err| err.to_string())?;
    }

    if popout_chrome_webview(app).is_none() {
        window
            .add_child(
                chrome_builder(BROWSER_CHROME_LABEL),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(width, POPOUT_CHROME_HEIGHT_LOGICAL),
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
pub async fn human_browser_embed_activate_tab(
    app: AppHandle,
    tab_id: String,
    url: String,
) -> Result<(), String> {
    activate_surface_tab(&app, BrowserSurface::Embed, tab_id.trim(), url.trim())
}

#[tauri::command]
pub async fn human_browser_embed_close_tab(app: AppHandle, tab_id: String) -> Result<(), String> {
    close_surface_tab(&app, BrowserSurface::Embed, tab_id.trim())
}

#[tauri::command]
pub async fn human_browser_popout_activate_tab(
    app: AppHandle,
    tab_id: String,
    url: String,
) -> Result<(), String> {
    activate_surface_tab(&app, BrowserSurface::Popout, tab_id.trim(), url.trim())
}

#[tauri::command]
pub async fn human_browser_popout_close_tab(app: AppHandle, tab_id: String) -> Result<(), String> {
    close_surface_tab(&app, BrowserSurface::Popout, tab_id.trim())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanBrowserNewWindowReport {
    pub url: String,
    #[serde(default = "default_embed_surface")]
    pub surface: String,
}

#[tauri::command]
pub fn human_browser_report_new_window(
    app: AppHandle,
    payload: HumanBrowserNewWindowReport,
) -> Result<(), String> {
    emit_new_window(
        &app,
        parse_surface(Some(payload.surface.as_str())),
        payload.url.trim(),
    );
    Ok(())
}

#[tauri::command]
pub async fn human_browser_navigate(app: AppHandle, url: String) -> Result<(), String> {
    let trimmed = url.trim().to_string();
    {
        let mut guard = surface_url_lock(BrowserSurface::Embed)
            .lock()
            .expect("embed active url");
        *guard = trimmed.clone();
    }

    if embedded_content_webview(&app).is_none() {
        // No webview yet — do not emit loading=true (nothing will emit Finished).
        // Frontend syncActiveTabToNative / activate creates the embed.
        return Ok(());
    }

    navigate_embedded_url(&app, &trimmed)
}

#[tauri::command]
pub async fn human_browser_popout_navigate(app: AppHandle, url: String) -> Result<(), String> {
    navigate_popout_url(&app, url.trim())
}

#[tauri::command]
pub async fn human_browser_reload(app: AppHandle) -> Result<(), String> {
    let url = human_browser_active_url();
    let trimmed = url.trim();
    if embedded_content_webview(&app).is_none() {
        return Ok(());
    }
    if trimmed.is_empty() || trimmed == "about:blank" {
        let content = embedded_content_webview(&app)
            .ok_or_else(|| "browser content webview not ready".to_string())?;
        content
            .eval("window.location.reload()")
            .map_err(|err| err.to_string())?;
        return Ok(());
    }
    navigate_embedded_url(&app, trimmed)
}

#[tauri::command]
pub async fn human_browser_popout_reload(app: AppHandle) -> Result<(), String> {
    let content = popout_content_webview(&app)
        .ok_or_else(|| "pop-out browser content not ready".to_string())?;
    content
        .eval("window.location.reload()")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_go_back(app: AppHandle) -> Result<(), String> {
    let content = embedded_content_webview(&app)
        .ok_or_else(|| "browser content webview not ready".to_string())?;
    content
        .eval("window.history.back()")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_popout_go_back(app: AppHandle) -> Result<(), String> {
    let content = popout_content_webview(&app)
        .ok_or_else(|| "pop-out browser content not ready".to_string())?;
    content
        .eval("window.history.back()")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_go_forward(app: AppHandle) -> Result<(), String> {
    let content = embedded_content_webview(&app)
        .ok_or_else(|| "browser content webview not ready".to_string())?;
    content
        .eval("window.history.forward()")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_popout_go_forward(app: AppHandle) -> Result<(), String> {
    let content = popout_content_webview(&app)
        .ok_or_else(|| "pop-out browser content not ready".to_string())?;
    content
        .eval("window.history.forward()")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn human_browser_report_title(
    app: AppHandle,
    url: String,
    title: String,
    surface: Option<String>,
) -> Result<(), String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    emit_navigated(
        &app,
        parse_surface(surface.as_deref()),
        url.trim(),
        Some(trimmed.to_string()),
        None,
        None,
    );
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
        embedded_content_webview(app).ok_or_else(|| "browser content webview not ready".to_string())?;
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
        emit_nav_state(
            &app,
            parse_surface(Some(payload.surface.as_str())),
            payload.can_go_back,
            payload.can_go_forward,
        );
    }
    Ok(())
}

#[tauri::command]
pub fn human_browser_report_favicon(
    url: String,
    favicon: String,
    surface: Option<String>,
) -> Result<(), String> {
    let trimmed = favicon.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if let Some(app) = app_handle() {
        emit_navigated(
            &app,
            parse_surface(surface.as_deref()),
            url.trim(),
            None,
            Some(trimmed.to_string()),
            None,
        );
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
    let content = embedded_content_webview(&app)
        .ok_or_else(|| "browser content webview not ready".to_string())?;
    emit_loading(&app, BrowserSurface::Embed, false);
    content
        .eval("try{window.stop();}catch(e){}")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_popout_stop(app: AppHandle) -> Result<(), String> {
    let content = popout_content_webview(&app)
        .ok_or_else(|| "pop-out browser content not ready".to_string())?;
    emit_loading(&app, BrowserSurface::Popout, false);
    content
        .eval("try{window.stop();}catch(e){}")
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn human_browser_query_nav_state(app: AppHandle) -> Result<HumanBrowserNavStatePayload, String> {
    let content = embedded_content_webview(&app)
        .ok_or_else(|| "browser content webview not ready".to_string())?;
    let (tx, rx) = oneshot::channel();
    *NAV_STATE_TX.lock().expect("nav state") = Some(tx);
    content
        .eval(
            r#"(function(){try{var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;i.invoke('human_browser_report_nav_state',{canGoBack:window.history.length>1,canGoForward:false,surface:'embed'});}catch(e){}})();"#,
        )
        .map_err(|err| err.to_string())?;
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .map_err(|_| "navigation state query timed out".to_string())?
        .map_err(|_| "navigation state channel closed".to_string())
}

#[tauri::command]
pub async fn human_browser_popout_query_nav_state(
    app: AppHandle,
) -> Result<HumanBrowserNavStatePayload, String> {
    let content = popout_content_webview(&app)
        .ok_or_else(|| "pop-out browser content not ready".to_string())?;
    let (tx, rx) = oneshot::channel();
    *NAV_STATE_TX.lock().expect("nav state") = Some(tx);
    content
        .eval(
            r#"(function(){try{var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;i.invoke('human_browser_report_nav_state',{canGoBack:window.history.length>1,canGoForward:false,surface:'popout'});}catch(e){}})();"#,
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
    let content = embedded_content_webview(&app)
        .ok_or_else(|| "browser content webview not ready".to_string())?;
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

#[tauri::command]
pub async fn human_browser_popout_find_in_page(
    app: AppHandle,
    query: String,
    forward: Option<bool>,
) -> Result<FindInPageResult, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(FindInPageResult { found: false });
    }
    let content = popout_content_webview(&app)
        .ok_or_else(|| "pop-out browser content not ready".to_string())?;
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
