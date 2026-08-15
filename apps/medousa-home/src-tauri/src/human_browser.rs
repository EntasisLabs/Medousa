//! Human-first browser: Rust-managed native webviews.
//!
//! **Embedded (primary):** `browser-content-embed-*` children on the main window, positioned
//! from the Web surface content pane. Chrome lives in Svelte (`HumanBrowserPanel`).
//!
//! **Pop-out (secondary):** `browser-content-popout` + `browser-chrome` on the dedicated
//! browser window — kept for a future "Pop out" action.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use medousa_browser_lite::{
    FetchResult, SearchResponse, markdown_from_html, search_response_from_ddg_html,
};
use serde::{Deserialize, Serialize};
use tauri::webview::{Color, DownloadEvent, NewWindowResponse, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, WebviewUrl};
use tokio::sync::oneshot;

const MAIN_WINDOW_LABEL: &str = "main";
/// Shell surface background — aligns WKWebView under-page compositing with the workshop chrome.
const EMBED_SURFACE_COLOR: Color = Color(12, 14, 18, 255);

const BROWSER_WINDOW_LABEL: &str = "browser";
const BROWSER_CONTENT_LABEL: &str = "browser-content-popout";
const BROWSER_CHROME_LABEL: &str = "browser-chrome";
const MAX_BROWSER_URL_BYTES: usize = 8 * 1024;
const MAX_BROWSER_TITLE_BYTES: usize = 512;
const MAX_BROWSER_REPORTS_PER_SECOND: u32 = 64;
const MAX_BROWSER_REQUEST_ID_BYTES: usize = 128;
/// Workshop-layout fallback only (desktop freeform uses DOM host measure).
/// Keep roughly in sync with AppTitlebar (~40) + browser toolbar (~36).
const CHROME_HEIGHT_LOGICAL: f64 = 96.0;
/// Pop-out chrome strip — must match `h-[132px]` in `popout/browser-chrome/+page.svelte`.
const POPOUT_CHROME_HEIGHT_LOGICAL: f64 = 132.0;

static POPOUT_SHELL_READY: AtomicBool = AtomicBool::new(false);
static EMBED_READY: AtomicBool = AtomicBool::new(false);
static EMBED_VISIBLE: AtomicBool = AtomicBool::new(false);
/// When true, show/flush should apply `LAST_EMBED_ACTIVE_URL` once (cold open / pre-create nav).
/// Cleared after flush or a successful embedded navigate — prevents show→navigate reload loops.
static EMBED_NAV_PENDING: AtomicBool = AtomicBool::new(false);
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

#[derive(Debug, Clone)]
struct BrowserWebviewIdentity {
    label: String,
    surface: BrowserSurface,
    tab_id: Option<String>,
    navigation_generation: u64,
    current_url: String,
    report_window_started: Instant,
    report_count: u32,
}

static BROWSER_WEBVIEWS: LazyLock<Mutex<HashMap<String, BrowserWebviewIdentity>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn register_browser_webview(label: &str, surface: BrowserSurface, tab_id: Option<String>) {
    if let Ok(mut webviews) = BROWSER_WEBVIEWS.lock() {
        webviews.insert(
            label.to_string(),
            BrowserWebviewIdentity {
                label: label.to_string(),
                surface,
                tab_id,
                navigation_generation: 0,
                current_url: "about:blank".to_string(),
                report_window_started: Instant::now(),
                report_count: 0,
            },
        );
    }
}

fn unregister_browser_webview(label: &str) {
    if let Ok(mut webviews) = BROWSER_WEBVIEWS.lock() {
        webviews.remove(label);
    }
}

fn browser_webview_identity(label: &str) -> Option<BrowserWebviewIdentity> {
    BROWSER_WEBVIEWS
        .lock()
        .ok()
        .and_then(|webviews| webviews.get(label).cloned())
}

fn begin_browser_navigation(label: &str, url: &str) -> Option<BrowserWebviewIdentity> {
    let mut webviews = BROWSER_WEBVIEWS.lock().ok()?;
    let identity = webviews.get_mut(label)?;
    identity.navigation_generation = identity.navigation_generation.saturating_add(1);
    identity.current_url.clear();
    identity.current_url.push_str(url);
    identity.report_window_started = Instant::now();
    identity.report_count = 0;
    Some(identity.clone())
}

fn finish_browser_navigation(label: &str, url: &str) -> Option<BrowserWebviewIdentity> {
    let mut webviews = BROWSER_WEBVIEWS.lock().ok()?;
    let identity = webviews.get_mut(label)?;
    identity.current_url.clear();
    identity.current_url.push_str(url);
    Some(identity.clone())
}

fn set_browser_tab_identity(label: &str, tab_id: Option<String>) {
    if let Ok(mut webviews) = BROWSER_WEBVIEWS.lock() {
        if let Some(identity) = webviews.get_mut(label) {
            identity.tab_id = tab_id;
        }
    }
}

fn admit_browser_report(label: &str) -> Result<BrowserWebviewIdentity, String> {
    let mut webviews = BROWSER_WEBVIEWS
        .lock()
        .map_err(|_| "browser webview registry is unavailable".to_string())?;
    let identity = webviews
        .get_mut(label)
        .ok_or_else(|| "browser webview is not registered".to_string())?;
    let now = Instant::now();
    if now.duration_since(identity.report_window_started) >= Duration::from_secs(1) {
        identity.report_window_started = now;
        identity.report_count = 0;
    }
    if identity.report_count >= MAX_BROWSER_REPORTS_PER_SECOND {
        return Err("browser report rate limit exceeded".to_string());
    }
    identity.report_count += 1;
    Ok(identity.clone())
}

fn validate_request_id(request_id: &str) -> Result<&str, String> {
    let request_id = request_id.trim();
    if request_id.is_empty()
        || request_id.len() > MAX_BROWSER_REQUEST_ID_BYTES
        || !request_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("invalid browser request id".to_string());
    }
    Ok(request_id)
}

fn is_allowed_browser_navigation(url: &url::Url) -> bool {
    if url.as_str().len() > MAX_BROWSER_URL_BYTES {
        return false;
    }
    matches!(url.scheme(), "http" | "https") || url.as_str() == "about:blank"
}

fn bounded_browser_title(title: &str) -> Option<String> {
    let mut bounded = String::with_capacity(title.len().min(MAX_BROWSER_TITLE_BYTES));
    for ch in title.trim().chars() {
        if ch.is_control() {
            continue;
        }
        if bounded.len() + ch.len_utf8() > MAX_BROWSER_TITLE_BYTES {
            break;
        }
        bounded.push(ch);
    }
    (!bounded.is_empty()).then_some(bounded)
}

fn surface_url_lock(surface: BrowserSurface) -> &'static Mutex<String> {
    match surface {
        BrowserSurface::Embed => LAST_EMBED_ACTIVE_URL.get_or_init(|| Mutex::new(String::new())),
        BrowserSurface::Popout => LAST_POPOUT_ACTIVE_URL.get_or_init(|| Mutex::new(String::new())),
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
        BrowserSurface::Embed => "browser-content-embed-",
        BrowserSurface::Popout => "browser-content-popout-",
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

fn close_all_tab_webviews(app: &AppHandle, surface: BrowserSurface) {
    BROWSER_HOST_STATE.advance_navigation(surface);
    let ids = tab_ids_lock(surface)
        .lock()
        .ok()
        .map(|guard| guard.clone())
        .unwrap_or_default();
    for id in ids {
        let label = tab_webview_label(surface, &id);
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.close();
        }
        unregister_browser_webview(&label);
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
    /// Same-document / History API change — update the URL bar without shell history push.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_page: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanBrowserPolicyBlockedPayload {
    action: &'static str,
    surface: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindInPageResult {
    pub found: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotReport {
    #[serde(default)]
    pub request_id: String,
    #[serde(default = "default_embed_surface")]
    pub surface: String,
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

const MAX_BROWSER_PENDING_REQUESTS: usize = 64;
const MAX_BROWSER_PENDING_PER_SURFACE: usize = 8;
const MAX_SNAPSHOT_REPORT_BYTES: usize = 8 * 1024 * 1024;
const MAX_BROWSER_CONTROL_REPORT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserRequestKind {
    Snapshot,
    Act,
    Navigation,
    Find,
}

enum BrowserPendingReply {
    Snapshot(oneshot::Sender<SnapshotReport>),
    Act(oneshot::Sender<BrowserActReport>),
    Navigation(oneshot::Sender<HumanBrowserNavStatePayload>),
    Find(oneshot::Sender<FindInPageResult>),
}

impl BrowserPendingReply {
    fn kind(&self) -> BrowserRequestKind {
        match self {
            Self::Snapshot(_) => BrowserRequestKind::Snapshot,
            Self::Act(_) => BrowserRequestKind::Act,
            Self::Navigation(_) => BrowserRequestKind::Navigation,
            Self::Find(_) => BrowserRequestKind::Find,
        }
    }
}

struct BrowserPendingRequest {
    webview_label: String,
    surface: BrowserSurface,
    navigation_generation: u64,
    reply: BrowserPendingReply,
}

#[derive(Default)]
struct BrowserBrokerCounters {
    matched: u64,
    late_or_unsolicited: u64,
    wrong_kind: u64,
    wrong_surface: u64,
    stale_navigation: u64,
    cancelled: u64,
    oversize: u64,
    capacity_rejected: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRequestDiagnostics {
    pub pending: usize,
    pub high_water: usize,
    pub matched: u64,
    pub late_or_unsolicited: u64,
    pub wrong_kind: u64,
    pub wrong_surface: u64,
    pub stale_navigation: u64,
    pub cancelled: u64,
    pub oversize: u64,
    pub capacity_rejected: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum BrowserPageReportV1 {
    Snapshot {
        request_id: String,
        html: String,
    },
    Action {
        request_id: String,
        ok: bool,
        #[serde(default)]
        error: Option<String>,
    },
    NavQuery {
        request_id: String,
        can_go_back: bool,
        can_go_forward: bool,
    },
    Find {
        request_id: String,
        found: bool,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct BrowserPageReport {
    version: u8,
    #[serde(flatten)]
    report: BrowserPageReportV1,
}

#[derive(Default)]
struct BrowserBrokerState {
    pending: HashMap<String, BrowserPendingRequest>,
    counters: BrowserBrokerCounters,
    high_water: usize,
}

struct BrowserHostState {
    next_request_id: AtomicU64,
    broker: Mutex<BrowserBrokerState>,
}

trait BrowserRequestIdentity {
    fn request_identity(self) -> BrowserWebviewIdentity;
}

impl BrowserRequestIdentity for &BrowserWebviewIdentity {
    fn request_identity(self) -> BrowserWebviewIdentity {
        self.clone()
    }
}

#[cfg(test)]
impl BrowserRequestIdentity for BrowserSurface {
    fn request_identity(self) -> BrowserWebviewIdentity {
        BrowserWebviewIdentity {
            label: format!("test-{}", self.as_str()),
            surface: self,
            tab_id: None,
            navigation_generation: 0,
            current_url: "https://example.test/".to_string(),
            report_window_started: Instant::now(),
            report_count: 0,
        }
    }
}

impl BrowserHostState {
    fn new() -> Self {
        Self {
            next_request_id: AtomicU64::new(1),
            broker: Mutex::new(BrowserBrokerState::default()),
        }
    }

    fn register<I: BrowserRequestIdentity>(
        &self,
        identity: I,
        reply: BrowserPendingReply,
    ) -> Result<String, String> {
        let identity = identity.request_identity();
        let mut state = self.broker.lock().expect("browser request broker");
        let surface = identity.surface;
        if state.pending.len() >= MAX_BROWSER_PENDING_REQUESTS {
            state.counters.capacity_rejected =
                state.counters.capacity_rejected.saturating_add(1);
            return Err(format!(
                "browser request capacity reached (limit {MAX_BROWSER_PENDING_REQUESTS})"
            ));
        }
        if state
            .pending
            .values()
            .filter(|pending| pending.surface == surface)
            .count()
            >= MAX_BROWSER_PENDING_PER_SURFACE
        {
            state.counters.capacity_rejected =
                state.counters.capacity_rejected.saturating_add(1);
            return Err(format!(
                "browser surface request capacity reached (limit {MAX_BROWSER_PENDING_PER_SURFACE})"
            ));
        }
        let sequence = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let request_id = format!("browser-{sequence}");
        state.pending.insert(
            request_id.clone(),
            BrowserPendingRequest {
                webview_label: identity.label.clone(),
                surface,
                navigation_generation: identity.navigation_generation,
                reply,
            },
        );
        state.high_water = state.high_water.max(state.pending.len());
        Ok(request_id)
    }

    fn take<I: BrowserRequestIdentity>(
        &self,
        request_id: &str,
        identity: I,
        kind: BrowserRequestKind,
    ) -> Option<BrowserPendingReply> {
        let identity = identity.request_identity();
        let mut state = self.broker.lock().expect("browser request broker");
        let Some(pending) = state.pending.get(request_id) else {
            state.counters.late_or_unsolicited =
                state.counters.late_or_unsolicited.saturating_add(1);
            return None;
        };
        if pending.reply.kind() != kind {
            state.counters.wrong_kind = state.counters.wrong_kind.saturating_add(1);
            return None;
        }
        if pending.surface != identity.surface || pending.webview_label != identity.label {
            state.counters.wrong_surface = state.counters.wrong_surface.saturating_add(1);
            return None;
        }
        let pending = state.pending.remove(request_id).expect("pending request exists");
        if pending.navigation_generation != identity.navigation_generation {
            state.counters.stale_navigation =
                state.counters.stale_navigation.saturating_add(1);
            return None;
        }
        state.counters.matched = state.counters.matched.saturating_add(1);
        Some(pending.reply)
    }

    fn cancel_request(&self, request_id: &str) {
        let mut state = self.broker.lock().expect("browser request broker");
        if state.pending.remove(request_id).is_some() {
            state.counters.cancelled = state.counters.cancelled.saturating_add(1);
        }
    }

    fn advance_navigation(&self, surface: BrowserSurface) {
        let mut state = self.broker.lock().expect("browser request broker");
        let before = state.pending.len();
        state.pending.retain(|_, pending| pending.surface != surface);
        state.counters.cancelled = state
            .counters
            .cancelled
            .saturating_add((before - state.pending.len()) as u64);
    }

    fn record_oversize(&self) {
        let mut state = self.broker.lock().expect("browser request broker");
        state.counters.oversize = state.counters.oversize.saturating_add(1);
    }

    fn diagnostics(&self) -> BrowserRequestDiagnostics {
        let state = self.broker.lock().expect("browser request broker");
        BrowserRequestDiagnostics {
            pending: state.pending.len(),
            high_water: state.high_water,
            matched: state.counters.matched,
            late_or_unsolicited: state.counters.late_or_unsolicited,
            wrong_kind: state.counters.wrong_kind,
            wrong_surface: state.counters.wrong_surface,
            stale_navigation: state.counters.stale_navigation,
            cancelled: state.counters.cancelled,
            oversize: state.counters.oversize,
            capacity_rejected: state.counters.capacity_rejected,
        }
    }
}

static BROWSER_HOST_STATE: LazyLock<BrowserHostState> = LazyLock::new(BrowserHostState::new);

fn request_identity<R: tauri::Runtime>(
    webview: &tauri::Webview<R>,
    expected_surface: BrowserSurface,
) -> Result<BrowserWebviewIdentity, String> {
    let identity = browser_webview_identity(webview.label())
        .ok_or_else(|| "browser webview is not registered".to_string())?;
    if identity.surface != expected_surface {
        return Err("browser webview surface mismatch".to_string());
    }
    let native_url = webview.url().map_err(|err| err.to_string())?;
    if !is_allowed_browser_navigation(&native_url) || native_url.as_str() == "about:blank" {
        return Err("browser webview has no reportable HTTP(S) document".to_string());
    }
    if identity.current_url != native_url.as_str() {
        return Err("browser webview navigation identity is stale".to_string());
    }
    Ok(identity)
}

pub(crate) fn accept_browser_page_report<R: tauri::Runtime>(
    webview: tauri::Webview<R>,
    message: BrowserPageReport,
) -> Result<(), String> {
    if message.version != 1 {
        return Err("unsupported browser report version".to_string());
    }
    let registered = browser_webview_identity(webview.label())
        .ok_or_else(|| "browser webview is not registered".to_string())?;
    let _ = request_identity(&webview, registered.surface)?;
    let identity = admit_browser_report(webview.label())?;
    let native_url = identity.current_url.clone();
    let surface = identity.surface.as_str().to_string();

    match message.report {
        BrowserPageReportV1::Snapshot { request_id, html } => {
            let validated_request_id = validate_request_id(&request_id)?;
            if html.len() > MAX_SNAPSHOT_REPORT_BYTES {
                BROWSER_HOST_STATE.cancel_request(validated_request_id);
                BROWSER_HOST_STATE.record_oversize();
                return Err(format!(
                    "snapshot exceeds {MAX_SNAPSHOT_REPORT_BYTES} byte limit"
                ));
            }
            if let Some(BrowserPendingReply::Snapshot(tx)) = BROWSER_HOST_STATE.take(
                validated_request_id,
                &identity,
                BrowserRequestKind::Snapshot,
            ) {
                let _ = tx.send(SnapshotReport {
                    request_id,
                    surface,
                    url: native_url,
                    html,
                });
            }
        }
        BrowserPageReportV1::Action {
            request_id,
            ok,
            error,
        } => {
            let validated_request_id = validate_request_id(&request_id)?;
            let report_bytes = request_id.len() + error.as_deref().map_or(0, str::len);
            if report_bytes > MAX_BROWSER_CONTROL_REPORT_BYTES {
                BROWSER_HOST_STATE.cancel_request(validated_request_id);
                BROWSER_HOST_STATE.record_oversize();
                return Err(format!(
                    "browser action report exceeds {MAX_BROWSER_CONTROL_REPORT_BYTES} byte limit"
                ));
            }
            if let Some(BrowserPendingReply::Act(tx)) = BROWSER_HOST_STATE.take(
                validated_request_id,
                &identity,
                BrowserRequestKind::Act,
            ) {
                let _ = tx.send(BrowserActReport {
                    request_id,
                    surface,
                    ok,
                    url: native_url,
                    error,
                });
            }
        }
        BrowserPageReportV1::NavQuery {
            request_id,
            can_go_back,
            can_go_forward,
        } => {
            let validated_request_id = validate_request_id(&request_id)?;
            let payload = HumanBrowserNavStatePayload {
                can_go_back,
                can_go_forward,
                surface,
                request_id: Some(request_id.clone()),
            };
            if let Some(BrowserPendingReply::Navigation(tx)) = BROWSER_HOST_STATE.take(
                validated_request_id,
                &identity,
                BrowserRequestKind::Navigation,
            ) {
                let _ = tx.send(payload.clone());
                if let Some(app) = app_handle() {
                    emit_nav_state(&app, identity.surface, can_go_back, can_go_forward);
                }
            }
        }
        BrowserPageReportV1::Find { request_id, found } => {
            let validated_request_id = validate_request_id(&request_id)?;
            if let Some(BrowserPendingReply::Find(tx)) = BROWSER_HOST_STATE.take(
                validated_request_id,
                &identity,
                BrowserRequestKind::Find,
            ) {
                let _ = tx.send(FindInPageResult { found });
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn human_browser_request_diagnostics() -> BrowserRequestDiagnostics {
    BROWSER_HOST_STATE.diagnostics()
}

struct BrowserPendingGuard<'a> {
    state: &'a BrowserHostState,
    request_id: String,
}

impl<'a> BrowserPendingGuard<'a> {
    fn new(state: &'a BrowserHostState, request_id: String) -> Self {
        Self { state, request_id }
    }
}

impl Drop for BrowserPendingGuard<'_> {
    fn drop(&mut self) {
        self.state.cancel_request(&self.request_id);
    }
}

pub fn init_app_handle(app: AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
    #[cfg(target_os = "macos")]
    install_macos_embed_hotkey_monitor(app);
}

pub fn app_handle() -> Option<AppHandle> {
    APP_HANDLE.get().cloned()
}

/// Map a chrome shortcut to a shell action. Caller already required Ctrl/Cmd.
fn map_chrome_hotkey_action(key: &str, shift: bool, alt: bool) -> Option<&'static str> {
    match (key, shift, alt) {
        ("l", false, false) => Some("focusUrl"),
        ("f", false, false) => Some("find"),
        ("b", true, false) => Some("bookmarks"),
        ("t", true, false) => Some("reopenTab"),
        ("t", false, false) => Some("newTab"),
        ("w", false, false) => Some("closeTab"),
        ("r", false, false) => Some("reload"),
        ("[", _, false) => Some("goBack"),
        ("]", _, false) => Some("goForward"),
        _ => None,
    }
}

fn dispatch_embed_hotkey(app: &AppHandle, action: &str, surface: &str) {
    if hotkey_needs_shell_focus(action) {
        if surface == "popout" {
            if let Some(chrome) = app.get_webview(BROWSER_CHROME_LABEL) {
                let _ = chrome.set_focus();
            }
        } else {
            focus_shell_webview(app);
        }
    }
    let _ = app.emit(
        "human-browser-hotkey",
        HumanBrowserHotkeyPayload {
            action: action.to_string(),
            surface: surface.to_string(),
        },
    );
}

/// Native macOS key monitor — page JS never sees chrome shortcuts while the embed
/// webview is visible (focus lives in WKWebView, outside the shell).
#[cfg(target_os = "macos")]
fn install_macos_embed_hotkey_monitor(app: AppHandle) {
    use std::sync::atomic::AtomicBool;
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }

    let app_for_block = app.clone();
    let _ = app.run_on_main_thread(move || {
        use std::ptr::NonNull;

        use block2::RcBlock;
        use objc2::rc::Retained;
        use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags};
        use objc2_foundation::NSString;

        let app_handle = app_for_block.clone();
        let block = RcBlock::new(move |event_ptr: NonNull<NSEvent>| -> *mut NSEvent {
            let event = unsafe { event_ptr.as_ref() };
            if event.isARepeat() {
                return event_ptr.as_ptr();
            }

            let flags = event.modifierFlags();
            let command = flags.contains(NSEventModifierFlags::Command);
            let control = flags.contains(NSEventModifierFlags::Control);
            if !command && !control {
                return event_ptr.as_ptr();
            }
            // Only steal chrome shortcuts while a page embed is on screen.
            // Start page / popovers hide the embed, so shell keydown owns those.
            if !EMBED_VISIBLE.load(Ordering::SeqCst) {
                return event_ptr.as_ptr();
            }

            let shift = flags.contains(NSEventModifierFlags::Shift);
            let option = flags.contains(NSEventModifierFlags::Option);
            let key = event
                .charactersIgnoringModifiers()
                .map(|s: Retained<NSString>| s.to_string().to_lowercase())
                .unwrap_or_default();

            let Some(action) = map_chrome_hotkey_action(key.as_str(), shift, option) else {
                return event_ptr.as_ptr();
            };

            dispatch_embed_hotkey(&app_handle, action, "embed");
            // Swallow so the page (and shell, if focused) don't also handle it.
            std::ptr::null_mut()
        });

        let monitor = unsafe {
            NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::KeyDown, &block)
        };
        // Keep the monitor (and its block) alive for the process lifetime.
        std::mem::forget(monitor);
        std::mem::forget(block);
    });
}

/// WebView2 only raises this while the page webview has focus — ideal for Windows.
#[cfg(windows)]
fn attach_windows_webview_hotkeys(
    webview: &tauri::Webview,
    surface: &'static str,
    require_embed_visible: bool,
) {
    let _ = webview.with_webview(move |platform| {
        use webview2_com::AcceleratorKeyPressedEventHandler;
        use webview2_com::Microsoft::Web::WebView2::Win32::{
            COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN, COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
            COREWEBVIEW2_PHYSICAL_KEY_STATUS, ICoreWebView2AcceleratorKeyPressedEventArgs,
            ICoreWebView2Controller,
        };
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetKeyState, VK_CONTROL, VK_MENU, VK_OEM_4, VK_OEM_6, VK_SHIFT,
        };

        let controller: ICoreWebView2Controller = platform.controller();
        let mut token = 0i64;
        let handler = AcceleratorKeyPressedEventHandler::create(Box::new(move |_sender, args| {
            if require_embed_visible && !EMBED_VISIBLE.load(Ordering::SeqCst) {
                return Ok(());
            }
            let Some(args) = args else {
                return Ok(());
            };
            let args: ICoreWebView2AcceleratorKeyPressedEventArgs = args;

            let mut kind = Default::default();
            unsafe { args.KeyEventKind(&mut kind)? };
            if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
            {
                return Ok(());
            }

            let mut status = COREWEBVIEW2_PHYSICAL_KEY_STATUS::default();
            unsafe { args.PhysicalKeyStatus(&mut status)? };
            if status.WasKeyDown.as_bool() {
                return Ok(());
            }

            let ctrl = unsafe { GetKeyState(VK_CONTROL.0 as i32) < 0 };
            if !ctrl {
                return Ok(());
            }
            let shift = unsafe { GetKeyState(VK_SHIFT.0 as i32) < 0 };
            let alt = unsafe { GetKeyState(VK_MENU.0 as i32) < 0 };

            let mut vk = 0u32;
            unsafe { args.VirtualKey(&mut vk)? };

            let key = if (0x41..=0x5A).contains(&vk) {
                ((vk as u8) as char).to_ascii_lowercase().to_string()
            } else if vk == VK_OEM_4.0 as u32 {
                "[".to_string()
            } else if vk == VK_OEM_6.0 as u32 {
                "]".to_string()
            } else {
                return Ok(());
            };

            let Some(action) = map_chrome_hotkey_action(key.as_str(), shift, alt) else {
                return Ok(());
            };
            unsafe { args.SetHandled(true)? };
            if let Some(handle) = app_handle() {
                dispatch_embed_hotkey(&handle, action, surface);
            }
            Ok(())
        }));

        let _ = unsafe { controller.add_AcceleratorKeyPressed(&handler, &mut token) };
        std::mem::forget(handler);
    });
}

/// GTK key-press on the page webview — fires only while that widget has focus.
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn attach_linux_webview_hotkeys(
    webview: &tauri::Webview,
    app: AppHandle,
    surface: &'static str,
    require_embed_visible: bool,
) {
    let _ = webview.with_webview(move |platform| {
        use gdk::ModifierType;
        use gtk::glib::Propagation;
        use gtk::prelude::*;

        let wv = platform.inner();
        let app_handle = app.clone();
        wv.connect_key_press_event(move |_w, event| {
            if require_embed_visible && !EMBED_VISIBLE.load(Ordering::SeqCst) {
                return Propagation::Proceed;
            }
            let state = event.state();
            let ctrl = state.contains(ModifierType::CONTROL_MASK);
            let meta = state.contains(ModifierType::META_MASK);
            if !ctrl && !meta {
                return Propagation::Proceed;
            }
            if event.is_modifier() {
                return Propagation::Proceed;
            }
            let shift = state.contains(ModifierType::SHIFT_MASK);
            let alt = state.contains(ModifierType::MOD1_MASK);
            let key = event
                .keyval()
                .to_unicode()
                .map(|c| c.to_lowercase().next().unwrap_or(c))
                .map(|c| c.to_string())
                .unwrap_or_default();

            let Some(action) = map_chrome_hotkey_action(key.as_str(), shift, alt) else {
                return Propagation::Proceed;
            };
            dispatch_embed_hotkey(&app_handle, action, surface);
            Propagation::Stop
        });
    });
}

fn attach_webview_chrome_hotkeys(
    webview: &tauri::Webview,
    app: &AppHandle,
    surface: BrowserSurface,
) {
    let require_embed_visible = matches!(surface, BrowserSurface::Embed);
    let surface_static: &'static str = match surface {
        BrowserSurface::Embed => "embed",
        BrowserSurface::Popout => "popout",
    };
    #[cfg(windows)]
    {
        let _ = app;
        attach_windows_webview_hotkeys(webview, surface_static, require_embed_visible);
    }
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        attach_linux_webview_hotkeys(webview, app.clone(), surface_static, require_embed_visible);
    }
    #[cfg(target_os = "macos")]
    let _ = (webview, app, surface_static, require_embed_visible);
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
    BROWSER_HOST_STATE.advance_navigation(BrowserSurface::Popout);
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
    let parsed = trimmed
        .parse()
        .map_err(|err: url::ParseError| err.to_string())?;
    if !is_allowed_browser_navigation(&parsed) || parsed.as_str() == "about:blank" {
        return Err("browser navigation requires a bounded HTTP(S) URL".to_string());
    }
    Ok(parsed)
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

fn emit_nav_state(
    app: &AppHandle,
    surface: BrowserSurface,
    can_go_back: bool,
    can_go_forward: bool,
) {
    let _ = app.emit(
        "human-browser-nav-state",
        HumanBrowserNavStatePayload {
            can_go_back,
            can_go_forward,
            surface: surface.as_str().to_string(),
            request_id: None,
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
    emit_navigated_ex(app, surface, url, title, favicon, tab_id, false);
}

fn emit_navigated_ex(
    app: &AppHandle,
    surface: BrowserSurface,
    url: &str,
    title: Option<String>,
    favicon: Option<String>,
    tab_id: Option<String>,
    in_page: bool,
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
        in_page,
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

fn content_builder(
    app: &AppHandle,
    label: String,
    mobile_ua: bool,
) -> WebviewBuilder<tauri::Wry> {
    let app_navigation = app.clone();
    let app_load = app.clone();
    let app_new_window = app.clone();
    let app_title = app.clone();
    let app_download = app.clone();
    let navigation_label = label.clone();
    let new_window_label = label.clone();
    let mut builder =
        WebviewBuilder::new(label, WebviewUrl::External("about:blank".parse().unwrap()))
            .on_new_window(move |url, _features| {
                let href = url.as_str();
                let Some(identity) = browser_webview_identity(&new_window_label) else {
                    return NewWindowResponse::Deny;
                };
                if !is_allowed_browser_navigation(&url) || href == "about:blank" {
                    return NewWindowResponse::Deny;
                }
                emit_new_window(&app_new_window, identity.surface, href);
                NewWindowResponse::Deny
            })
            .on_navigation(move |nav_url| {
                let Some(identity) = browser_webview_identity(&navigation_label) else {
                    return false;
                };
                if !is_allowed_browser_navigation(nav_url) {
                    let _ = app_navigation.emit(
                        "human-browser-policy-blocked",
                        HumanBrowserPolicyBlockedPayload {
                            action: "navigation",
                            surface: identity.surface.as_str().to_string(),
                        },
                    );
                    return false;
                }
                BROWSER_HOST_STATE.advance_navigation(identity.surface);
                let _ = begin_browser_navigation(&navigation_label, nav_url.as_str());
                emit_loading(&app_navigation, identity.surface, true);
                true
            })
            .on_page_load(move |webview, payload| {
                use tauri::webview::PageLoadEvent;
                let Some(identity) = browser_webview_identity(webview.label()) else {
                    return;
                };
                match payload.event() {
                    PageLoadEvent::Started => emit_loading(&app_load, identity.surface, true),
                    PageLoadEvent::Finished => {
                        emit_loading(&app_load, identity.surface, false);
                        let href = payload.url().as_str().to_string();
                        if !is_allowed_browser_navigation(payload.url()) {
                            return;
                        }
                        let identity = finish_browser_navigation(webview.label(), &href)
                            .unwrap_or(identity);
                        emit_navigated(
                            &app_load,
                            identity.surface,
                            &href,
                            None,
                            None,
                            identity.tab_id,
                        );
                        if mobile_ua {
                            let _ = webview.eval(MOBILE_EMBED_FIX_JS);
                        } else {
                            let _ = webview.eval(DESKTOP_EMBED_FILL_JS);
                        }
                    }
                }
            })
            .on_document_title_changed(move |webview, title| {
                let Some(identity) = browser_webview_identity(webview.label()) else {
                    return;
                };
                let Some(title) = bounded_browser_title(&title) else {
                    return;
                };
                let url = webview
                    .url()
                    .ok()
                    .filter(is_allowed_browser_navigation)
                    .map(|url| url.to_string())
                    .unwrap_or(identity.current_url);
                emit_navigated(
                    &app_title,
                    identity.surface,
                    &url,
                    Some(title),
                    None,
                    identity.tab_id,
                );
            })
            .on_download(move |webview, event| {
                if matches!(event, DownloadEvent::Requested { .. }) {
                    if let Some(identity) = browser_webview_identity(webview.label()) {
                        let _ = app_download.emit(
                            "human-browser-policy-blocked",
                            HumanBrowserPolicyBlockedPayload {
                                action: "download",
                                surface: identity.surface.as_str().to_string(),
                            },
                        );
                    }
                }
                false
            });
    if mobile_ua {
        builder = builder.user_agent(MOBILE_SAFARI_UA);
    } else {
        builder = builder.background_color(EMBED_SURFACE_COLOR);
    }
    builder
}

fn chrome_builder(label: &'static str) -> WebviewBuilder<tauri::Wry> {
    WebviewBuilder::new(label, WebviewUrl::App("/popout/browser-chrome".into()))
}

fn default_embed_layout() -> EmbedLayoutParams {
    EmbedLayoutParams {
        // Activity rail was removed from WorkshopShell; keep the field for
        // wire compatibility but never reserve horizontal space for it.
        activity_width: 0.0,
        activity_collapsed: true,
        work_rail_visible: false,
        content_top: None,
    }
}

/// Fixed Rust layout for the embedded pane — last-resort fallback when Freeform
/// DOM bounds are unavailable. Mirrors current shell chrome (no activity rail).
fn compute_embedded_bounds(
    window: &tauri::Window,
    params: EmbedLayoutParams,
) -> Result<EmbedBounds, String> {
    let (win_w, win_h) = window_inner_logical(window)?;
    let bottom_chrome = STATUS_BAR_HEIGHT
        + if params.work_rail_visible {
            WORK_RAIL_HEIGHT
        } else {
            0.0
        };
    let chrome_top = params
        .content_top
        .filter(|value| *value > 0.0)
        .unwrap_or(CHROME_HEIGHT_LOGICAL);

    // Do not reserve a fixed nav rail or activity strip — MasterRail width is
    // dynamic (0 when collapsed). Freeform DOM measure owns precise placement.
    Ok(EmbedBounds {
        x: 0.0,
        y: chrome_top,
        width: win_w.max(8.0),
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

/// DOM `getBoundingClientRect` is in the shell webview layout viewport (top-left).
/// Tauri child `set_bounds` uses the window contentView (top-left, y=0 at view top).
///
/// Pre-Overlay macOS: the shell WKWebView sat *below* the title bar, so DOM y=0 was
/// already inset and we still had to add `contentLayoutRect` when converting.
/// Overlay + fullSizeContentView: the shell webview is full-bleed from contentView
/// (0,0) and AppTitlebar is in-flow HTML — DOM rects are already contentView coords.
/// Adding the title-bar inset again shifts the embed down (~28–40px gap under chrome).
fn macos_dom_to_content_view_adjust(app: &AppHandle, shell_x: f64, shell_y: f64) -> (f64, f64) {
    let (viewport_inset_x, viewport_inset_y) =
        macos_shell_viewport_origin_in_window(app).unwrap_or((0.0, 0.0));
    // Full-bleed shell webview: skip contentLayoutRect inset.
    if shell_x.abs() < 1.0 && shell_y.abs() < 1.0 {
        return (0.0, 0.0);
    }
    (viewport_inset_x, viewport_inset_y)
}

fn dom_bounds_to_window_child_bounds(
    app: &AppHandle,
    dom: EmbedBounds,
) -> Result<EmbedBounds, String> {
    let (shell_x, shell_y) = shell_webview_origin(app)?;
    let (adj_x, adj_y) = macos_dom_to_content_view_adjust(app, shell_x, shell_y);
    Ok(EmbedBounds {
        x: dom.x + shell_x + adj_x,
        y: dom.y + shell_y + adj_y,
        width: dom.width,
        height: dom.height,
    })
}

/// Keep child webviews inside the workshop window so a slightly oversized DOM
/// measure (100vw / flex overflow / zoom) cannot paint under shell chrome.
fn clamp_embed_bounds_to_window(app: &AppHandle, bounds: EmbedBounds) -> Result<EmbedBounds, String> {
    let window = workshop_window(app)?;
    let (win_w, win_h) = window_inner_logical(&window)?;
    let x = bounds.x.clamp(0.0, (win_w - 8.0).max(0.0));
    let y = bounds.y.clamp(0.0, (win_h - 8.0).max(0.0));
    let width = bounds.width.max(8.0).min((win_w - x).max(8.0));
    let height = bounds.height.max(8.0).min((win_h - y).max(8.0));
    Ok(EmbedBounds {
        x,
        y,
        width,
        height,
    })
}

fn window_child_bounds_to_dom_bounds(app: &AppHandle, window_bounds: EmbedBounds) -> EmbedBounds {
    let (shell_x, shell_y) = shell_webview_origin(app).unwrap_or((0.0, 0.0));
    let (adj_x, adj_y) = macos_dom_to_content_view_adjust(app, shell_x, shell_y);
    EmbedBounds {
        x: window_bounds.x - shell_x - adj_x,
        y: window_bounds.y - shell_y - adj_y,
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
unsafe fn macos_rect_top_left_in_view(
    rect: objc2_foundation::NSRect,
    view: &objc2_app_kit::NSView,
) -> f64 {
    if view.isFlipped() {
        rect.origin.y
    } else {
        view.bounds().size.height - rect.origin.y - rect.size.height
    }
}

#[cfg(target_os = "macos")]
fn macos_webview_frame_in_window_content(webview: &tauri::Webview) -> Option<(f64, f64, f64, f64)> {
    use std::sync::{Arc, Mutex};
    let out = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&out);
    let _ = webview.with_webview(move |w| unsafe {
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
    });
    Arc::try_unwrap(out).ok()?.into_inner().ok().flatten()
}

/// wry `set_bounds` checks `isFlipped()` but `bounds()` readback always assumes non-flipped.
#[cfg(target_os = "macos")]
fn macos_webview_layout_diagnostics(webview: &tauri::Webview) -> Option<serde_json::Value> {
    use std::sync::{Arc, Mutex};
    let out = Arc::new(Mutex::new(None));
    let capture = Arc::clone(&out);
    let _ = webview.with_webview(move |w| unsafe {
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
fn macos_webview_frame_in_window_content(
    _webview: &tauri::Webview,
) -> Option<(f64, f64, f64, f64)> {
    None
}

fn bounds_json(x: f64, y: f64, w: f64, h: f64) -> serde_json::Value {
    serde_json::json!({ "x": x, "y": y, "width": w, "height": h, "bottom": y + h, "right": x + w })
}

fn coordinate_frame_snapshot(
    app: &AppHandle,
    dom: Option<EmbedBounds>,
) -> Result<serde_json::Value, String> {
    let window = workshop_window(app)?;
    let (win_w, win_h) = window_inner_logical(&window)?;
    let scale = window.scale_factor().map_err(|err| err.to_string())?;
    let (shell_x, shell_y) = shell_webview_origin(app)?;
    let (main_x, main_y, main_w, main_h) = webview_tauri_bounds_logical(app, MAIN_WINDOW_LABEL)?;
    let workshop = compute_embedded_bounds(&window, default_embed_layout())?;

    let content_tauri = embedded_content_webview(app)
        .map(|content| {
            let rect = content.bounds().ok();
            rect.map(|rect| {
                let pos = rect.position.to_logical::<f64>(scale);
                let size = rect.size.to_logical::<f64>(scale);
                bounds_json(pos.x, pos.y, size.width, size.height)
            })
        })
        .flatten();

    let main_native = app
        .get_webview(MAIN_WINDOW_LABEL)
        .and_then(|wv| macos_webview_frame_in_window_content(&wv))
        .map(|(x, y, w, h)| bounds_json(x, y, w, h));

    let content_native = embedded_content_webview(app)
        .and_then(|wv| macos_webview_frame_in_window_content(&wv))
        .map(|(x, y, w, h)| bounds_json(x, y, w, h));

    let dom_target = dom
        .map(|d| dom_bounds_to_window_child_bounds(app, d).ok())
        .flatten();

    let dom_vs_content_native = match (dom_target, content_native.as_ref()) {
        (Some(target), Some(native)) => Some(serde_json::json!({
            "x": target.x - native["x"].as_f64().unwrap_or(0.0),
            "y": target.y - native["y"].as_f64().unwrap_or(0.0),
            "w": target.width - native["width"].as_f64().unwrap_or(0.0),
            "h": target.height - native["height"].as_f64().unwrap_or(0.0),
        })),
        _ => None,
    };

    let macos_diagnostics =
        embedded_content_webview(app).and_then(|wv| macos_webview_layout_diagnostics(&wv));

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
            content.set_bounds(rect).map_err(|err| err.to_string())?;
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
    let target = clamp_embed_bounds_to_window(app, dom_bounds_to_window_child_bounds(app, dom)?)?;
    apply_embedded_bounds(app, target)?;

    // Gap chase is macOS-only (flipped contentView / title-bar inset mismatch).
    // On Windows/Linux, repeated set_bounds during load can abort navigation (reload spam).
    #[cfg(target_os = "macos")]
    {
        for _ in 0..2 {
            let Some(content) = embedded_content_webview(app) else {
                return Ok(());
            };

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

            let gap_x = target.x - actual_x;
            let gap_y = target.y - actual_y;
            let gap_w = target.width - actual_w;
            let gap_h = target.height - actual_h;

            if gap_x.abs() <= 2.0 && gap_y.abs() <= 2.0 && gap_w.abs() <= 2.0 && gap_h.abs() <= 2.0
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
fn embed_stored_bounds_to_window(
    app: &AppHandle,
    bounds: EmbedBounds,
) -> Result<EmbedBounds, String> {
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
    register_browser_webview(
        &label,
        BrowserSurface::Embed,
        Some(tab_id.to_string()),
    );
    if let Err(err) = window
        .add_child(
            content_builder(
                app,
                label.clone(),
                mobile_ua,
            ),
            LogicalPosition::new(x, y),
            LogicalSize::new(width.max(8.0), height.max(8.0)),
        )
    {
        unregister_browser_webview(&label);
        return Err(err.to_string());
    }
    register_tab_id(BrowserSurface::Embed, tab_id);
    EMBED_READY.store(true, Ordering::SeqCst);
    if let Some(created) = tab_webview(app, BrowserSurface::Embed, tab_id) {
        attach_webview_chrome_hotkeys(&created, app, BrowserSurface::Embed);
    }
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
    // Drop freeform placement so a later show cannot revive pre-split full-pane bounds
    // while the shell compositor is still measuring the new tile host.
    if let Ok(mut last) = LAST_EMBED_PLACEMENT.lock() {
        *last = None;
    }
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
}

fn activate_embed_tab(app: &AppHandle, tab_id: &str, initial_url: &str) -> Result<(), String> {
    if active_tab_id(BrowserSurface::Embed).as_deref() != Some(tab_id) {
        BROWSER_HOST_STATE.advance_navigation(BrowserSurface::Embed);
    }

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
        EMBED_NAV_PENDING.store(false, Ordering::SeqCst);
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
        EMBED_NAV_PENDING.store(false, Ordering::SeqCst);
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

fn activate_popout_tab(app: &AppHandle, tab_id: &str, initial_url: &str) -> Result<(), String> {
    ensure_popout_shell(app)?;

    if active_tab_id(BrowserSurface::Popout).as_deref() != Some(tab_id) {
        BROWSER_HOST_STATE.advance_navigation(BrowserSurface::Popout);
    }

    {
        let mut guard = active_tab_id_lock(BrowserSurface::Popout)
            .lock()
            .map_err(|_| "active tab lock poisoned".to_string())?;
        *guard = Some(tab_id.to_string());
    }
    set_browser_tab_identity(BROWSER_CONTENT_LABEL, Some(tab_id.to_string()));

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
            let label = tab_webview_label(BrowserSurface::Embed, tab_id);
            if let Some(webview) = app.get_webview(&label) {
                webview.close().map_err(|err| err.to_string())?;
            }
            unregister_browser_webview(&label);
            unregister_tab_id(BrowserSurface::Embed, tab_id);
        }
        BrowserSurface::Popout => {
            // Pop-out uses a single shared content webview — only drop tab metadata.
            set_browser_tab_identity(BROWSER_CONTENT_LABEL, None);
        }
    }
    if active_tab_id(surface).as_deref() == Some(tab_id) {
        BROWSER_HOST_STATE.advance_navigation(surface);
        if let Ok(mut guard) = active_tab_id_lock(surface).lock() {
            *guard = None;
        }
    }
    Ok(())
}

fn flush_pending_embed_navigation(app: &AppHandle) {
    // Only apply a URL that was queued before the embed existed. Calling navigate on every
    // show/layout (especially Windows WebView2) aborts in-flight loads → reload spam.
    if !EMBED_NAV_PENDING.swap(false, Ordering::SeqCst) {
        return;
    }
    let url = human_browser_active_url();
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        return;
    }
    let Some(tab_id) = active_tab_id(BrowserSurface::Embed) else {
        EMBED_NAV_PENDING.store(true, Ordering::SeqCst);
        return;
    };
    if tab_webview(app, BrowserSurface::Embed, &tab_id).is_none() {
        match activate_surface_tab(app, BrowserSurface::Embed, &tab_id, trimmed) {
            Ok(()) => EMBED_NAV_PENDING.store(false, Ordering::SeqCst),
            Err(_) => EMBED_NAV_PENDING.store(true, Ordering::SeqCst),
        }
        return;
    }
    if navigate_tab_webview(app, BrowserSurface::Embed, trimmed, false).is_err() {
        EMBED_NAV_PENDING.store(true, Ordering::SeqCst);
    }
}

fn navigate_embedded_url(app: &AppHandle, url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    {
        let mut guard = surface_url_lock(BrowserSurface::Embed)
            .lock()
            .expect("embed active url");
        *guard = trimmed.to_string();
    }
    // Same-URL is a no-op (reload uses location.reload). force:true was restarting loads
    // whenever show/flush raced an in-flight navigation.
    let result = navigate_tab_webview(app, BrowserSurface::Embed, trimmed, false);
    if result.is_ok() {
        EMBED_NAV_PENDING.store(false, Ordering::SeqCst);
    }
    result
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
    let was_visible = EMBED_VISIBLE.swap(true, Ordering::SeqCst);
    let placement = LAST_EMBED_PLACEMENT.lock().ok().and_then(|guard| *guard);
    match placement {
        Some(EmbedPlacement::Freeform(bounds)) => {
            if was_visible {
                // Already composited — compositor owns set_bounds; avoid gap-chase + nav flush.
                if let Some(content) = embedded_content_webview(&app) {
                    content.show().map_err(|err| err.to_string())?;
                }
            } else {
                apply_embedded_dom_bounds(&app, bounds)?;
            }
        }
        Some(_) => {
            if !was_visible {
                reapply_embedded_placement(&app)?;
            } else if let Some(content) = embedded_content_webview(&app) {
                content.show().map_err(|err| err.to_string())?;
            }
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
        register_browser_webview(BROWSER_CONTENT_LABEL, BrowserSurface::Popout, None);
        if let Err(err) = window
            .add_child(
                content_builder(
                    app,
                    BROWSER_CONTENT_LABEL.to_string(),
                    false,
                ),
                LogicalPosition::new(0.0, POPOUT_CHROME_HEIGHT_LOGICAL),
                LogicalSize::new(width, content_height),
            )
        {
            unregister_browser_webview(BROWSER_CONTENT_LABEL);
            return Err(err.to_string());
        }
        if let Some(content) = popout_content_webview(app) {
            attach_webview_chrome_hotkeys(&content, app, BrowserSurface::Popout);
        }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanBrowserHotkeyPayload {
    pub action: String,
    #[serde(default = "default_embed_surface")]
    pub surface: String,
}

fn focus_shell_webview(app: &AppHandle) {
    if let Some(shell) = app.get_webview(MAIN_WINDOW_LABEL) {
        let _ = shell.set_focus();
    }
}

fn hotkey_needs_shell_focus(action: &str) -> bool {
    matches!(action, "focusUrl" | "find" | "bookmarks")
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
        // No webview yet — queue for flush on first show (do not emit loading=true).
        EMBED_NAV_PENDING.store(true, Ordering::SeqCst);
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
    if embedded_content_webview(&app).is_none() {
        return Ok(());
    }
    let content = embedded_content_webview(&app)
        .ok_or_else(|| "browser content webview not ready".to_string())?;
    content.reload().map_err(|err| err.to_string())?;
    emit_loading(&app, BrowserSurface::Embed, true);
    Ok(())
}

#[tauri::command]
pub async fn human_browser_popout_reload(app: AppHandle) -> Result<(), String> {
    let content = popout_content_webview(&app)
        .ok_or_else(|| "pop-out browser content not ready".to_string())?;
    content.reload().map_err(|err| err.to_string())
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

fn snapshot_capture_js(request_id: &str) -> Result<String, String> {
    let request_id = serde_json::to_string(request_id).map_err(|err| err.to_string())?;
    Ok(format!(r#"(function(){{
try{{
  var html=document.documentElement?document.documentElement.outerHTML:"";
  var url=window.location.href||"";
  var i=window.__TAURI_INTERNALS__||window.__TAURI__;
  if(!i||!i.invoke)return;
  i.invoke("plugin:browser-bridge|report",{{report:{{version:1,kind:"snapshot",requestId:{request_id},html:html}}}});
}}catch(e){{}}
}})();"#))
}

const ACT_REPORT_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserActReport {
    #[serde(default)]
    pub request_id: String,
    #[serde(default = "default_embed_surface")]
    pub surface: String,
    pub ok: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub error: Option<String>,
}

fn browser_act_js(request: &BrowserActRequest, request_id: &str) -> Result<String, String> {
    let payload = serde_json::to_string(request).map_err(|err| err.to_string())?;
    let request_id = serde_json::to_string(request_id).map_err(|err| err.to_string())?;
    Ok(format!(
        r#"(function(){{
try{{
  var req={payload};
  var requestId={request_id};
  function done(ok,error){{
    var i=window.__TAURI_INTERNALS__||window.__TAURI__;
    if(!i||!i.invoke)return;
    i.invoke("plugin:browser-bridge|report",{{report:{{version:1,kind:"action",requestId:requestId,ok:ok,error:error||null}}}});
  }}
  function visible(el){{
    if(!el)return false;
    var r=el.getBoundingClientRect();
    return r.width>0&&r.height>0;
  }}
  var el=req.selector?document.querySelector(req.selector):null;
  if(req.selector&&!el){{done(false,"no element matches selector: "+req.selector);return;}}
  if(req.selector&&!visible(el)){{done(false,"target element is not visible: "+req.selector);return;}}
  switch(req.action){{
    case "click": el.click(); done(true); return;
    case "type":
      el.focus();
      if("value" in el){{el.value=req.text||"";el.dispatchEvent(new Event("input",{{bubbles:true}}));el.dispatchEvent(new Event("change",{{bubbles:true}}));done(true);return;}}
      if(el.isContentEditable){{el.textContent=req.text||"";el.dispatchEvent(new Event("input",{{bubbles:true}}));done(true);return;}}
      done(false,"target does not accept text input"); return;
    case "press":
      var key=req.key||"Enter";
      ["keydown","keypress","keyup"].forEach(function(t){{el.dispatchEvent(new KeyboardEvent(t,{{key:key,bubbles:true}}));}});
      if(key==="Enter"&&el.form&&el.tagName!=="TEXTAREA"){{el.form.requestSubmit?el.form.requestSubmit():el.form.submit();}}
      done(true); return;
    case "scroll":
      if(el){{el.scrollIntoView({{block:"center",behavior:"instant"}});}}
      else{{window.scrollBy(0,req.delta_y||400);}}
      done(true); return;
    case "select":
      if(el.tagName!=="SELECT"){{done(false,"target is not a <select>");return;}}
      el.value=req.value||"";
      el.dispatchEvent(new Event("change",{{bubbles:true}}));
      done(true); return;
    case "wait":
      setTimeout(function(){{done(true);}},Math.min(Math.max(req.ms||1000,0),15000));
      return;
    default: done(false,"unsupported action: "+req.action); return;
  }}
}}catch(e){{try{{var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(i&&i.invoke)i.invoke("plugin:browser-bridge|report",{{report:{{version:1,kind:"action",requestId:requestId,ok:false,error:String(e)}}}});}}catch(_err){{}}}}
}})();"#
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BrowserActRequest {
    pub action: String,
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub delta_y: Option<i64>,
    #[serde(default)]
    pub ms: Option<u64>,
}

pub async fn browser_act_embed(
    app: &AppHandle,
    request: &BrowserActRequest,
) -> Result<BrowserActReport, String> {
    let content = embedded_content_webview(app)
        .ok_or_else(|| "browser content webview not ready".to_string())?;
    let identity = request_identity(&content, BrowserSurface::Embed)?;
    let (tx, rx) = oneshot::channel();
    let request_id = BROWSER_HOST_STATE.register(
        &identity,
        BrowserPendingReply::Act(tx),
    )?;
    let _guard = BrowserPendingGuard::new(&BROWSER_HOST_STATE, request_id.clone());
    content
        .eval(&browser_act_js(request, &request_id)?)
        .map_err(|err| err.to_string())?;
    tokio::time::timeout(ACT_REPORT_TIMEOUT, rx)
        .await
        .map_err(|_| "browser act timed out waiting for page".to_string())?
        .map_err(|_| "browser act channel closed".to_string())
}

async fn capture_html(app: &AppHandle) -> Result<SnapshotReport, String> {
    let content = embedded_content_webview(app)
        .ok_or_else(|| "browser content webview not ready".to_string())?;
    let identity = request_identity(&content, BrowserSurface::Embed)?;
    let (tx, rx) = oneshot::channel();
    let request_id = BROWSER_HOST_STATE.register(
        &identity,
        BrowserPendingReply::Snapshot(tx),
    )?;
    let _guard = BrowserPendingGuard::new(&BROWSER_HOST_STATE, request_id.clone());
    content
        .eval(&snapshot_capture_js(&request_id)?)
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
pub async fn human_browser_query_nav_state(
    app: AppHandle,
) -> Result<HumanBrowserNavStatePayload, String> {
    let content = embedded_content_webview(&app)
        .ok_or_else(|| "browser content webview not ready".to_string())?;
    let identity = request_identity(&content, BrowserSurface::Embed)?;
    let (tx, rx) = oneshot::channel();
    let request_id = BROWSER_HOST_STATE.register(
        &identity,
        BrowserPendingReply::Navigation(tx),
    )?;
    let _guard = BrowserPendingGuard::new(&BROWSER_HOST_STATE, request_id.clone());
    let request_id = serde_json::to_string(&request_id).map_err(|err| err.to_string())?;
    content
        .eval(&format!(
            r#"(function(){{try{{var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;i.invoke('plugin:browser-bridge|report',{{report:{{version:1,kind:'navQuery',requestId:{request_id},canGoBack:window.history.length>1,canGoForward:false}}}});}}catch(e){{}}}})();"#,
        ))
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
    let identity = request_identity(&content, BrowserSurface::Popout)?;
    let (tx, rx) = oneshot::channel();
    let request_id = BROWSER_HOST_STATE.register(
        &identity,
        BrowserPendingReply::Navigation(tx),
    )?;
    let _guard = BrowserPendingGuard::new(&BROWSER_HOST_STATE, request_id.clone());
    let request_id = serde_json::to_string(&request_id).map_err(|err| err.to_string())?;
    content
        .eval(&format!(
            r#"(function(){{try{{var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;i.invoke('plugin:browser-bridge|report',{{report:{{version:1,kind:'navQuery',requestId:{request_id},canGoBack:window.history.length>1,canGoForward:false}}}});}}catch(e){{}}}})();"#,
        ))
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
    let identity = request_identity(&content, BrowserSurface::Embed)?;
    let (tx, rx) = oneshot::channel();
    let request_id = BROWSER_HOST_STATE.register(
        &identity,
        BrowserPendingReply::Find(tx),
    )?;
    let _guard = BrowserPendingGuard::new(&BROWSER_HOST_STATE, request_id.clone());
    let forward_lit = if forward.unwrap_or(true) {
        "true"
    } else {
        "false"
    };
    let query_json = serde_json::to_string(trimmed).map_err(|err| err.to_string())?;
    let request_id = serde_json::to_string(&request_id).map_err(|err| err.to_string())?;
    let script = format!(
        r#"(function(){{try{{var q={query_json};var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;var found=window.find(q,false,{forward_lit},true,false,true,false);i.invoke('plugin:browser-bridge|report',{{report:{{version:1,kind:'find',requestId:{request_id},found:!!found}}}});}}catch(e){{}}}})();"#
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
    let identity = request_identity(&content, BrowserSurface::Popout)?;
    let (tx, rx) = oneshot::channel();
    let request_id = BROWSER_HOST_STATE.register(
        &identity,
        BrowserPendingReply::Find(tx),
    )?;
    let _guard = BrowserPendingGuard::new(&BROWSER_HOST_STATE, request_id.clone());
    let forward_lit = if forward.unwrap_or(true) {
        "true"
    } else {
        "false"
    };
    let query_json = serde_json::to_string(trimmed).map_err(|err| err.to_string())?;
    let request_id = serde_json::to_string(&request_id).map_err(|err| err.to_string())?;
    let script = format!(
        r#"(function(){{try{{var q={query_json};var i=window.__TAURI_INTERNALS__||window.__TAURI__;if(!i||!i.invoke)return;var found=window.find(q,false,{forward_lit},true,false,true,false);i.invoke('plugin:browser-bridge|report',{{report:{{version:1,kind:'find',requestId:{request_id},found:!!found}}}});}}catch(e){{}}}})();"#
    );
    content.eval(&script).map_err(|err| err.to_string())?;
    tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .map_err(|_| "find in page timed out".to_string())?
        .map_err(|_| "find in page channel closed".to_string())
}

#[cfg(test)]
mod request_broker_tests {
    use super::*;

    #[test]
    fn remote_navigation_policy_is_http_only_with_bounded_blank_bootstrap() {
        for allowed in [
            "about:blank",
            "https://example.test/path",
            "http://127.0.0.1:8080/",
        ] {
            assert!(is_allowed_browser_navigation(
                &url::Url::parse(allowed).unwrap()
            ));
        }
        for denied in [
            "file:///tmp/secret",
            "data:text/html,owned",
            "javascript:alert(1)",
            "tauri://localhost/",
            "medousa://settings",
        ] {
            assert!(!is_allowed_browser_navigation(
                &url::Url::parse(denied).unwrap()
            ));
        }

        let oversized = format!("https://example.test/{}", "a".repeat(MAX_BROWSER_URL_BYTES));
        assert!(!is_allowed_browser_navigation(
            &url::Url::parse(&oversized).unwrap()
        ));
    }

    #[test]
    fn native_titles_are_control_free_and_bounded_before_shell_delivery() {
        let title = format!("  hello\nworld\0{}  ", "x".repeat(MAX_BROWSER_TITLE_BYTES));
        let bounded = bounded_browser_title(&title).unwrap();
        assert!(!bounded.chars().any(char::is_control));
        assert!(bounded.len() <= MAX_BROWSER_TITLE_BYTES);
        assert!(bounded.starts_with("helloworld"));
        assert_eq!(bounded_browser_title(" \n\0 "), None);
    }

    #[test]
    fn registry_binds_surface_and_generation_to_native_webview_label() {
        let label = "browser-content-embed-registry-test";
        register_browser_webview(label, BrowserSurface::Embed, Some("tab-a".into()));
        let first = begin_browser_navigation(label, "https://example.test/one").unwrap();
        let second = begin_browser_navigation(label, "https://example.test/two").unwrap();
        assert_eq!(first.surface, BrowserSurface::Embed);
        assert_eq!(first.tab_id.as_deref(), Some("tab-a"));
        assert_eq!(first.navigation_generation + 1, second.navigation_generation);
        assert_eq!(second.current_url, "https://example.test/two");
        unregister_browser_webview(label);
        assert!(browser_webview_identity(label).is_none());
    }

    #[test]
    fn report_envelope_is_versioned_and_closed() {
        let report: BrowserPageReport = serde_json::from_value(serde_json::json!({
            "version": 1,
            "kind": "find",
            "requestId": "browser-1",
            "found": true
        }))
        .unwrap();
        assert!(matches!(report.report, BrowserPageReportV1::Find { found: true, .. }));

        assert!(serde_json::from_value::<BrowserPageReport>(serde_json::json!({
            "version": 1,
            "kind": "find",
            "requestId": "browser-1",
            "found": true,
            "authority": "shell"
        }))
        .is_err());
        assert!(serde_json::from_value::<BrowserPageReport>(serde_json::json!({
            "version": 1,
            "kind": "hotkey",
            "requestId": "browser-1"
        }))
        .is_err());
    }

    #[test]
    fn broker_rejects_same_surface_wrong_webview_and_generation() {
        let state = BrowserHostState::new();
        let identity = BrowserWebviewIdentity {
            label: "browser-content-embed-a".to_string(),
            surface: BrowserSurface::Embed,
            tab_id: Some("a".to_string()),
            navigation_generation: 7,
            current_url: "https://example.test/".to_string(),
            report_window_started: Instant::now(),
            report_count: 0,
        };
        let (tx, _rx) = oneshot::channel();
        let request_id = state
            .register(&identity, BrowserPendingReply::Find(tx))
            .unwrap();

        let mut forged = identity.clone();
        forged.label = "browser-content-embed-b".to_string();
        assert!(state
            .take(&request_id, &forged, BrowserRequestKind::Find)
            .is_none());
        assert_eq!(state.diagnostics().pending, 1);

        let mut stale = identity.clone();
        stale.navigation_generation += 1;
        assert!(state
            .take(&request_id, &stale, BrowserRequestKind::Find)
            .is_none());
        assert_eq!(state.diagnostics().pending, 0);
        assert_eq!(state.diagnostics().stale_navigation, 1);
    }

    fn snapshot(request_id: &str, marker: &str) -> SnapshotReport {
        SnapshotReport {
            request_id: request_id.to_string(),
            surface: "embed".to_string(),
            url: format!("https://example.test/{marker}"),
            html: marker.to_string(),
        }
    }

    #[tokio::test]
    async fn overlapping_snapshot_responses_match_exact_request_ids() {
        let state = BrowserHostState::new();
        let (first_tx, first_rx) = oneshot::channel();
        let first_id = state
            .register(
                BrowserSurface::Embed,
                BrowserPendingReply::Snapshot(first_tx),
            )
            .unwrap();
        let (second_tx, second_rx) = oneshot::channel();
        let second_id = state
            .register(
                BrowserSurface::Embed,
                BrowserPendingReply::Snapshot(second_tx),
            )
            .unwrap();

        let second = snapshot(&second_id, "second");
        if let Some(BrowserPendingReply::Snapshot(tx)) = state.take(
            &second_id,
            BrowserSurface::Embed,
            BrowserRequestKind::Snapshot,
        ) {
            tx.send(second).unwrap();
        }
        let first = snapshot(&first_id, "first");
        if let Some(BrowserPendingReply::Snapshot(tx)) = state.take(
            &first_id,
            BrowserSurface::Embed,
            BrowserRequestKind::Snapshot,
        ) {
            tx.send(first).unwrap();
        }

        assert_eq!(first_rx.await.unwrap().html, "first");
        assert_eq!(second_rx.await.unwrap().html, "second");
        assert_eq!(state.diagnostics().pending, 0);
    }

    #[tokio::test]
    async fn wrong_kind_cannot_consume_another_request() {
        let state = BrowserHostState::new();
        let (tx, rx) = oneshot::channel();
        let request_id = state
            .register(
                BrowserSurface::Embed,
                BrowserPendingReply::Snapshot(tx),
            )
            .unwrap();

        assert!(
            state
                .take(
                    &request_id,
                    BrowserSurface::Embed,
                    BrowserRequestKind::Find,
                )
                .is_none()
        );
        assert_eq!(state.diagnostics().pending, 1);
        if let Some(BrowserPendingReply::Snapshot(tx)) = state.take(
            &request_id,
            BrowserSurface::Embed,
            BrowserRequestKind::Snapshot,
        ) {
            tx.send(snapshot(&request_id, "matched")).unwrap();
        }
        assert_eq!(rx.await.unwrap().html, "matched");
    }

    #[tokio::test]
    async fn every_reply_kind_can_overlap_and_complete_in_reverse_order() {
        let state = BrowserHostState::new();
        let (act_tx, act_rx) = oneshot::channel();
        let act_id = state
            .register(BrowserSurface::Embed, BrowserPendingReply::Act(act_tx))
            .unwrap();
        let (nav_tx, nav_rx) = oneshot::channel();
        let nav_id = state
            .register(
                BrowserSurface::Popout,
                BrowserPendingReply::Navigation(nav_tx),
            )
            .unwrap();
        let (find_tx, find_rx) = oneshot::channel();
        let find_id = state
            .register(BrowserSurface::Embed, BrowserPendingReply::Find(find_tx))
            .unwrap();

        if let Some(BrowserPendingReply::Find(tx)) = state.take(
            &find_id,
            BrowserSurface::Embed,
            BrowserRequestKind::Find,
        ) {
            tx.send(FindInPageResult { found: true }).unwrap();
        }
        if let Some(BrowserPendingReply::Navigation(tx)) = state.take(
            &nav_id,
            BrowserSurface::Popout,
            BrowserRequestKind::Navigation,
        ) {
            tx.send(HumanBrowserNavStatePayload {
                can_go_back: true,
                can_go_forward: false,
                surface: "popout".to_string(),
                request_id: Some(nav_id.clone()),
            })
            .unwrap();
        }
        if let Some(BrowserPendingReply::Act(tx)) = state.take(
            &act_id,
            BrowserSurface::Embed,
            BrowserRequestKind::Act,
        ) {
            tx.send(BrowserActReport {
                request_id: act_id,
                surface: "embed".to_string(),
                ok: true,
                url: "https://example.test".to_string(),
                error: None,
            })
            .unwrap();
        }

        assert!(find_rx.await.unwrap().found);
        assert!(nav_rx.await.unwrap().can_go_back);
        assert!(act_rx.await.unwrap().ok);
        assert_eq!(state.diagnostics().pending, 0);
    }

    #[tokio::test]
    async fn wrong_surface_cannot_consume_the_target_request() {
        let state = BrowserHostState::new();
        let (tx, rx) = oneshot::channel();
        let request_id = state
            .register(BrowserSurface::Embed, BrowserPendingReply::Find(tx))
            .unwrap();

        assert!(
            state
                .take(
                    &request_id,
                    BrowserSurface::Popout,
                    BrowserRequestKind::Find,
                )
                .is_none()
        );
        assert_eq!(state.diagnostics().pending, 1);
        if let Some(BrowserPendingReply::Find(tx)) = state.take(
            &request_id,
            BrowserSurface::Embed,
            BrowserRequestKind::Find,
        ) {
            tx.send(FindInPageResult { found: true }).unwrap();
        }
        assert!(rx.await.unwrap().found);
        assert_eq!(state.diagnostics().pending, 0);
    }

    #[tokio::test]
    async fn dropped_request_guard_removes_the_pending_sender() {
        let state = BrowserHostState::new();
        let (tx, rx) = oneshot::channel();
        let request_id = state
            .register(BrowserSurface::Embed, BrowserPendingReply::Find(tx))
            .unwrap();
        let guard = BrowserPendingGuard::new(&state, request_id);

        drop(guard);

        assert_eq!(state.diagnostics().pending, 0);
        assert!(rx.await.is_err());
    }

    #[tokio::test]
    async fn navigation_cancels_only_the_stale_surface_generation() {
        let state = BrowserHostState::new();
        let (embed_tx, embed_rx) = oneshot::channel();
        state
            .register(
                BrowserSurface::Embed,
                BrowserPendingReply::Find(embed_tx),
            )
            .unwrap();
        let (popout_tx, _popout_rx) = oneshot::channel();
        state
            .register(
                BrowserSurface::Popout,
                BrowserPendingReply::Find(popout_tx),
            )
            .unwrap();

        state.advance_navigation(BrowserSurface::Embed);

        assert!(embed_rx.await.is_err());
        let diagnostics = state.diagnostics();
        assert_eq!(diagnostics.pending, 1);
        assert_eq!(diagnostics.cancelled, 1);
    }

    #[test]
    fn pending_request_admission_is_bounded() {
        let state = BrowserHostState::new();
        let mut receivers = Vec::new();
        for _ in 0..MAX_BROWSER_PENDING_PER_SURFACE {
            let (tx, rx) = oneshot::channel();
            state
                .register(BrowserSurface::Embed, BrowserPendingReply::Find(tx))
                .unwrap();
            receivers.push(rx);
        }
        let (overflow_tx, _overflow_rx) = oneshot::channel();
        assert!(
            state
                .register(
                    BrowserSurface::Embed,
                    BrowserPendingReply::Find(overflow_tx),
                )
                .is_err()
        );
        let diagnostics = state.diagnostics();
        assert_eq!(diagnostics.pending, MAX_BROWSER_PENDING_PER_SURFACE);
        assert_eq!(diagnostics.high_water, MAX_BROWSER_PENDING_PER_SURFACE);
        assert_eq!(diagnostics.capacity_rejected, 1);
        drop(receivers);
    }

    #[test]
    fn diagnostics_count_match_mismatch_cancel_and_oversize_paths() {
        let state = BrowserHostState::new();
        let (tx, _rx) = oneshot::channel();
        let request_id = state
            .register(BrowserSurface::Embed, BrowserPendingReply::Find(tx))
            .unwrap();

        assert!(
            state
                .take(
                    &request_id,
                    BrowserSurface::Embed,
                    BrowserRequestKind::Snapshot,
                )
                .is_none()
        );
        state.record_oversize();
        state.cancel_request(&request_id);
        assert!(
            state
                .take(
                    &request_id,
                    BrowserSurface::Embed,
                    BrowserRequestKind::Find,
                )
                .is_none()
        );

        let diagnostics = state.diagnostics();
        assert_eq!(diagnostics.pending, 0);
        assert_eq!(diagnostics.high_water, 1);
        assert_eq!(diagnostics.wrong_kind, 1);
        assert_eq!(diagnostics.cancelled, 1);
        assert_eq!(diagnostics.oversize, 1);
        assert_eq!(diagnostics.late_or_unsolicited, 1);
    }
}
