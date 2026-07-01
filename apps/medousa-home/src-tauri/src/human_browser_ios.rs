//! iOS human browser — UIKit WKWebView overlay (Tauri `add_child` is desktop-only).
//!
//! Uses raw WebKit bindings because `objc2-web-kit`'s typed `WKWebView` is macOS-only.

use std::ffi::CStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::TryRecvError;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use block2::{ManualBlockEncoding, RcBlock};
use medousa_browser_lite::{markdown_from_html, search_response_from_ddg_html, SearchResponse};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{msg_send, MainThreadMarker};
use objc2_core_foundation::{CGFloat, CGRect, CGPoint, CGSize};
use objc2_foundation::{
    NSDate, NSDefaultRunLoopMode, NSRunLoop, NSString, NSURL, NSURLRequest,
};
use objc2_ui_kit::{UIApplication, UIView, UIViewController, UIWindow};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const OVERLAY_TAG: isize = 0x4d_45_44_00;
const MOBILE_BROWSER_CHROME_FALLBACK: f64 = 52.0;
const MOBILE_SAFARI_UA: &str =
    "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";

const NEW_WINDOW_INSTALL_JS: &str = r#"(function(){if(window.__medousaNewWindowInstalled)return;window.__medousaNewWindowInstalled=true;function q(u){if(!u||u==='about:blank')return;document.documentElement.setAttribute('data-medousa-new-window',u)}var o=window.open;window.open=function(u){q(u);return null};document.addEventListener('click',function(e){var a=e.target.closest&&e.target.closest('a[target="_blank"]');if(a&&a.href){e.preventDefault();q(a.href)}},true)})();"#;

const NEW_WINDOW_POLL_JS: &str = r#"(function(){var u=document.documentElement.getAttribute('data-medousa-new-window');if(!u)return null;document.documentElement.removeAttribute('data-medousa-new-window');return u})();"#;

const SNAPSHOT_JS: &str = r#"(function(){try{var html=document.documentElement?document.documentElement.outerHTML:"";var url=window.location.href||"";return JSON.stringify({url:url,html:html});}catch(e){return JSON.stringify({url:"",html:""});}})();"#;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanBrowserNewWindowPayload {
    url: String,
}

fn emit_new_window(app: &AppHandle, url: &str) {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        return;
    }
    let _ = app.emit(
        "human-browser-new-window",
        HumanBrowserNewWindowPayload {
            url: trimmed.to_string(),
        },
    );
}

fn poll_pending_new_window(mtm: MainThreadMarker) -> Result<Option<String>, String> {
    let raw = eval_js_sync(mtm, NEW_WINDOW_POLL_JS)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(None);
    }
    if trimmed.starts_with('"') && trimmed.ends_with('"') {
        return Ok(serde_json::from_str(&trimmed).ok());
    }
    Ok(Some(trimmed.to_string()))
}

fn install_new_window_hooks(mtm: MainThreadMarker) -> Result<(), String> {
    let _ = eval_js_sync(mtm, NEW_WINDOW_INSTALL_JS)?;
    Ok(())
}

static MOBILE_SHELL_ACTIVE: AtomicBool = AtomicBool::new(false);
static EMBED_VISIBLE: AtomicBool = AtomicBool::new(false);
static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
static LAST_ACTIVE_URL: OnceLock<Mutex<String>> = OnceLock::new();
static LAST_BOUNDS: Mutex<Option<EmbedBounds>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedMobileLayoutParams {
    pub bottom_chrome_height: f64,
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
pub struct EmbedBoundsReadback {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub window_width: f64,
    pub window_height: f64,
}

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HumanBrowserNavigatedPayload {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    favicon: Option<String>,
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

pub fn init_app_handle(app: AppHandle) {
    let _ = APP_HANDLE.set(app);
}

fn active_url_lock() -> &'static Mutex<String> {
    LAST_ACTIVE_URL.get_or_init(|| Mutex::new(String::new()))
}

fn run_on_main<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce(MainThreadMarker) -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    APP_HANDLE
        .get()
        .ok_or_else(|| "app handle not initialized".to_string())?
        .run_on_main_thread(move || {
            let _ = tx.send(match MainThreadMarker::new() {
                Some(mtm) => f(mtm),
                None => Err("human browser requires main thread".to_string()),
            });
        })
        .map_err(|err| err.to_string())?;
    rx.recv()
        .map_err(|_| "main thread channel closed".to_string())?
}

fn root_view(mtm: MainThreadMarker) -> Result<Retained<UIView>, String> {
    let app = UIApplication::sharedApplication(mtm);
    let window: Retained<UIWindow> = app
        .keyWindow()
        .ok_or_else(|| "no key window".to_string())?;
    let controller: Retained<UIViewController> = window
        .rootViewController()
        .ok_or_else(|| "no root view controller".to_string())?;
    controller
        .view()
        .ok_or_else(|| "no root view".to_string())
}

fn window_size(mtm: MainThreadMarker) -> Result<(f64, f64), String> {
    let app = UIApplication::sharedApplication(mtm);
    let window = app
        .keyWindow()
        .ok_or_else(|| "no key window".to_string())?;
    let bounds = window.bounds();
    Ok((bounds.size.width as f64, bounds.size.height as f64))
}

fn cg_rect_from_bounds(bounds: EmbedBounds) -> CGRect {
    CGRect {
        origin: CGPoint::new(bounds.x as CGFloat, bounds.y as CGFloat),
        size: CGSize::new(
            bounds.width.max(8.0) as CGFloat,
            bounds.height.max(8.0) as CGFloat,
        ),
    }
}

struct JsEvalCompletionEncoding;

unsafe impl ManualBlockEncoding for JsEvalCompletionEncoding {
    type Arguments = (*mut AnyObject, *mut AnyObject);
    type Return = ();

    const ENCODING_CSTR: &'static CStr = if cfg!(target_pointer_width = "64") {
        c"v24@?0@8@16"
    } else {
        c"v12@?0@4@8"
    };
}

fn pump_main_runloop(seconds: f64) {
    let run_loop = NSRunLoop::currentRunLoop();
    let limit = NSDate::dateWithTimeIntervalSinceNow(seconds);
    unsafe {
        let _ = run_loop.runMode_beforeDate(NSDefaultRunLoopMode, &limit);
    }
}

fn wk_webview_class() -> Result<&'static AnyClass, String> {
    AnyClass::get(CStr::from_bytes_with_nul(b"WKWebView\0").unwrap())
        .ok_or_else(|| "WKWebView class unavailable".to_string())
}

fn wk_config_class() -> Result<&'static AnyClass, String> {
    AnyClass::get(CStr::from_bytes_with_nul(b"WKWebViewConfiguration\0").unwrap())
        .ok_or_else(|| "WKWebViewConfiguration class unavailable".to_string())
}

fn overlay_webview(parent: &UIView) -> Option<Retained<AnyObject>> {
    parent.viewWithTag(OVERLAY_TAG).and_then(|view| {
        let ptr: *mut AnyObject = view.as_ref() as *const UIView as *mut AnyObject;
        unsafe { Retained::retain(ptr) }
    })
}

fn compute_mobile_bounds(
    params: EmbedMobileLayoutParams,
    win_w: f64,
    win_h: f64,
) -> EmbedBounds {
    if let Some(measured) = params.content_bounds {
        return EmbedBounds {
            x: measured.x,
            y: measured.y,
            width: measured.width.max(8.0),
            height: measured.height.max(8.0),
        };
    }
    let bottom = params.bottom_chrome_height.max(0.0);
    EmbedBounds {
        x: 0.0,
        y: 0.0,
        width: win_w.max(8.0),
        height: (win_h - MOBILE_BROWSER_CHROME_FALLBACK - bottom).max(8.0),
    }
}

unsafe fn create_overlay_webview(frame: CGRect) -> Result<Retained<AnyObject>, String> {
    let config_class = wk_config_class()?;
    let config: *mut AnyObject = msg_send![config_class, new];
    let ua = NSString::from_str(MOBILE_SAFARI_UA);
    let _: () = msg_send![config, setApplicationNameForUserAgent: &*ua];

    let webview_class = wk_webview_class()?;
    let webview_alloc: *mut AnyObject = msg_send![webview_class, alloc];
    let webview: *mut AnyObject =
        msg_send![webview_alloc, initWithFrame: frame, configuration: config];
    let _: () = msg_send![webview, setCustomUserAgent: &*ua];
    Retained::retain(webview).ok_or_else(|| "failed to retain WKWebView".to_string())
}

fn ensure_overlay_webview(mtm: MainThreadMarker) -> Result<Retained<AnyObject>, String> {
    let parent = root_view(mtm)?;
    if let Some(existing) = overlay_webview(&parent) {
        return Ok(existing);
    }

    let bounds = LAST_BOUNDS
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .unwrap_or(EmbedBounds {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 480.0,
        });

    let frame = cg_rect_from_bounds(bounds);
    let webview = unsafe { create_overlay_webview(frame)? };
    unsafe {
        let _: () = msg_send![&*webview, setTag: OVERLAY_TAG];
        let parent_view: &UIView = &parent;
        let _: () = msg_send![parent_view, addSubview: Retained::as_ptr(&webview)];
    }
    let _ = install_new_window_hooks(mtm);
    Ok(webview)
}

fn apply_bounds(mtm: MainThreadMarker, bounds: EmbedBounds) -> Result<(), String> {
    if let Ok(mut last) = LAST_BOUNDS.lock() {
        *last = Some(bounds);
    }
    let webview = ensure_overlay_webview(mtm)?;
    let frame = cg_rect_from_bounds(bounds);
    unsafe {
        let _: () = msg_send![&*webview, setFrame: frame];
    }
    Ok(())
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

fn emit_navigated(
    app: &AppHandle,
    url: &str,
    title: Option<String>,
    favicon: Option<String>,
) {
    if let Ok(mut guard) = active_url_lock().lock() {
        *guard = url.to_string();
    }
    let payload = HumanBrowserNavigatedPayload {
        url: url.to_string(),
        title,
        favicon,
    };
    let _ = app.emit("human-browser-navigated", payload);
}

fn read_nav_state(webview: &AnyObject) -> (bool, bool) {
    unsafe {
        let can_back: Bool = msg_send![webview, canGoBack];
        let can_forward: Bool = msg_send![webview, canGoForward];
        (can_back.as_bool(), can_forward.as_bool())
    }
}

fn read_page_title(webview: &AnyObject) -> Option<String> {
    unsafe {
        let title: Option<Retained<NSString>> = msg_send![webview, title];
        title.map(|value| value.to_string())
    }
}

fn read_page_url(webview: &AnyObject) -> String {
    unsafe {
        let url: Option<Retained<NSURL>> = msg_send![webview, URL];
        url.and_then(|value| value.absoluteString())
            .map(|value| value.to_string())
            .unwrap_or_default()
    }
}

fn schedule_navigated_poll(app: AppHandle) {
    emit_loading(&app, true);
    tauri::async_runtime::spawn(async move {
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let app_clone = app.clone();
            let result = run_on_main(move |mtm| {
                let parent = root_view(mtm)?;
                let Some(webview) = overlay_webview(&parent) else {
                    return Ok(None);
                };
                let loading: Bool = unsafe { msg_send![&*webview, isLoading] };
                if loading.as_bool() {
                    return Ok(None);
                }
                let url = read_page_url(&webview);
                let title = read_page_title(&webview);
                let nav = read_nav_state(&webview);
                Ok(Some((url, title, nav)))
            });
            if let Ok(Some((url, title, nav))) = result {
                emit_loading(&app_clone, false);
                emit_nav_state(&app_clone, nav.0, nav.1);
                if !url.is_empty() && url != "about:blank" {
                    emit_navigated(&app_clone, &url, title, None);
                }
                let _ = run_on_main(move |mtm| {
                    let _ = install_new_window_hooks(mtm);
                    if let Ok(Some(pending)) = poll_pending_new_window(mtm) {
                        emit_new_window(&app_clone, &pending);
                    }
                    Ok(())
                });
                break;
            }
            let _ = run_on_main(move |mtm| {
                if let Ok(Some(pending)) = poll_pending_new_window(mtm) {
                    emit_new_window(&app_clone, &pending);
                }
                Ok(())
            });
        }
        emit_loading(&app, false);
    });
}

fn load_url(mtm: MainThreadMarker, url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed == "about:blank" {
        return Ok(());
    }
    let parsed = trimmed
        .parse::<url::Url>()
        .map_err(|err| err.to_string())?;
    let webview = ensure_overlay_webview(mtm)?;
    let ns_url = NSURL::URLWithString(&NSString::from_str(parsed.as_str()))
        .ok_or_else(|| format!("invalid url: {trimmed}"))?;
    let request = NSURLRequest::requestWithURL(&ns_url);
    unsafe {
        let _: *mut AnyObject = msg_send![&*webview, loadRequest: &*request];
    }
    Ok(())
}

fn eval_js_sync(_mtm: MainThreadMarker, js: &str) -> Result<String, String> {
    let webview = ensure_overlay_webview(_mtm)?;
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let js_string = NSString::from_str(js);

    let block = RcBlock::with_encoding::<_, _, _, JsEvalCompletionEncoding>(
        move |value: *mut AnyObject, error: *mut AnyObject| {
            if !error.is_null() {
                let _ = tx.send(Err("javascript evaluation failed".to_string()));
                return;
            }
            if value.is_null() {
                let _ = tx.send(Ok(String::new()));
                return;
            }
            unsafe {
                if let Some(obj) = Retained::retain(value) {
                    let string = unsafe { Retained::cast_unchecked::<NSString>(obj) };
                    let _ = tx.send(Ok(string.to_string()));
                    return;
                }
            }
            let _ = tx.send(Ok(String::new()));
        },
    );

    unsafe {
        let _: () = msg_send![
            &*webview,
            evaluateJavaScript: &*js_string,
            completionHandler: &*block
        ];
    }

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match rx.try_recv() {
            Ok(result) => return result,
            Err(TryRecvError::Disconnected) => {
                return Err("javascript completion channel closed".to_string());
            }
            Err(TryRecvError::Empty) => {}
        }
        if Instant::now() >= deadline {
            return Err("javascript evaluation timed out".to_string());
        }
        pump_main_runloop(0.05);
    }
}

async fn capture_html_async(app: &AppHandle) -> Result<SnapshotReport, String> {
    let _ = app;
    tauri::async_runtime::spawn_blocking(move || {
        let raw = run_on_main(|mtm| eval_js_sync(mtm, SNAPSHOT_JS))?;
        if raw.is_empty() {
            return Err("empty snapshot".to_string());
        }
        let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|err| err.to_string())?;
        Ok(SnapshotReport {
            url: parsed
                .get("url")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
            html: parsed
                .get("html")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string(),
        })
    })
    .await
    .map_err(|err| err.to_string())?
}

#[tauri::command]
pub async fn human_browser_navigate(app: AppHandle, url: String) -> Result<(), String> {
    let trimmed = url.trim().to_string();
    let for_load = trimmed.clone();
    run_on_main(move |mtm| load_url(mtm, &for_load))?;
    if !trimmed.is_empty() && trimmed != "about:blank" {
        if let Ok(mut guard) = active_url_lock().lock() {
            *guard = trimmed.clone();
        }
        emit_navigated(&app, &trimmed, None, None);
    }
    schedule_navigated_poll(app);
    Ok(())
}

#[tauri::command]
pub async fn human_browser_reload(app: AppHandle) -> Result<(), String> {
    run_on_main(|mtm| {
        let webview = ensure_overlay_webview(mtm)?;
        unsafe {
            let _: *mut AnyObject = msg_send![&*webview, reload];
        }
        Ok(())
    })?;
    schedule_navigated_poll(app);
    Ok(())
}

#[tauri::command]
pub async fn human_browser_go_back(app: AppHandle) -> Result<(), String> {
    run_on_main(|mtm| {
        let webview = ensure_overlay_webview(mtm)?;
        unsafe {
            let can_back: Bool = msg_send![&*webview, canGoBack];
            if can_back.as_bool() {
                let _: *mut AnyObject = msg_send![&*webview, goBack];
            }
        }
        Ok(())
    })?;
    schedule_navigated_poll(app);
    Ok(())
}

#[tauri::command]
pub async fn human_browser_go_forward(app: AppHandle) -> Result<(), String> {
    run_on_main(|mtm| {
        let webview = ensure_overlay_webview(mtm)?;
        unsafe {
            let can_forward: Bool = msg_send![&*webview, canGoForward];
            if can_forward.as_bool() {
                let _: *mut AnyObject = msg_send![&*webview, goForward];
            }
        }
        Ok(())
    })?;
    schedule_navigated_poll(app);
    Ok(())
}

#[tauri::command]
pub fn human_browser_embed_apply_layout(
    _app: AppHandle,
    _params: EmbedLayoutParams,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn human_browser_embed_apply_mobile_layout(
    _app: AppHandle,
    params: EmbedMobileLayoutParams,
) -> Result<bool, String> {
    MOBILE_SHELL_ACTIVE.store(true, Ordering::SeqCst);
    let had_overlay = run_on_main(|mtm| {
        let parent = root_view(mtm)?;
        Ok(overlay_webview(&parent).is_some())
    })?;
    run_on_main(move |mtm| {
        let (win_w, win_h) = window_size(mtm)?;
        let bounds = compute_mobile_bounds(params, win_w, win_h);
        apply_bounds(mtm, bounds)?;
        EMBED_VISIBLE.store(true, Ordering::SeqCst);
        let webview = ensure_overlay_webview(mtm)?;
        unsafe {
            let _: () = msg_send![&*webview, setHidden: Bool::NO];
        }
        Ok(())
    })?;
    Ok(!had_overlay)
}

#[tauri::command]
pub fn human_browser_embed_set_bounds(_app: AppHandle, bounds: EmbedBoundsDto) -> Result<(), String> {
    run_on_main(move |mtm| apply_bounds(mtm, bounds.into()))
}

#[tauri::command]
pub fn human_browser_embed_show(_app: AppHandle) -> Result<(), String> {
    run_on_main(|mtm| {
        let webview = ensure_overlay_webview(mtm)?;
        unsafe {
            let _: () = msg_send![&*webview, setHidden: Bool::NO];
        }
        EMBED_VISIBLE.store(true, Ordering::SeqCst);
        Ok(())
    })
}

#[tauri::command]
pub fn human_browser_embed_hide(_app: AppHandle) -> Result<(), String> {
    run_on_main(|mtm| {
        let parent = root_view(mtm)?;
        if let Some(webview) = overlay_webview(&parent) {
            unsafe {
                let _: () = msg_send![&*webview, setHidden: Bool::YES];
            }
        }
        EMBED_VISIBLE.store(false, Ordering::SeqCst);
        Ok(())
    })
}

#[tauri::command]
pub fn human_browser_embed_read_bounds(_app: AppHandle) -> Result<EmbedBoundsReadback, String> {
    run_on_main(|mtm| {
        let (win_w, win_h) = window_size(mtm)?;
        let parent = root_view(mtm)?;
        if let Some(webview) = overlay_webview(&parent) {
            let frame: CGRect = unsafe { msg_send![&*webview, frame] };
            return Ok(EmbedBoundsReadback {
                x: frame.origin.x as f64,
                y: frame.origin.y as f64,
                width: frame.size.width as f64,
                height: frame.size.height as f64,
                window_width: win_w,
                window_height: win_h,
            });
        }
        Ok(EmbedBoundsReadback {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            window_width: win_w,
            window_height: win_h,
        })
    })
}

#[tauri::command]
pub fn human_browser_set_mobile_shell_active(active: bool) {
    MOBILE_SHELL_ACTIVE.store(active, Ordering::SeqCst);
}

#[tauri::command]
pub fn human_browser_report_title(app: AppHandle, url: String, title: String) -> Result<(), String> {
    emit_navigated(&app, url.trim(), Some(title.trim().to_string()), None);
    Ok(())
}

#[tauri::command]
pub fn human_browser_report_favicon(url: String, favicon: String) -> Result<(), String> {
    let trimmed = favicon.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if let Some(app) = APP_HANDLE.get() {
        emit_navigated(app, url.trim(), None, Some(trimmed.to_string()));
    }
    Ok(())
}

#[tauri::command]
pub fn human_browser_report_nav_state(payload: HumanBrowserNavStatePayload) -> Result<(), String> {
    if let Some(app) = APP_HANDLE.get() {
        emit_nav_state(app, payload.can_go_back, payload.can_go_forward);
    }
    Ok(())
}

#[tauri::command]
pub async fn human_browser_stop(app: AppHandle) -> Result<(), String> {
    run_on_main(|mtm| {
        let webview = ensure_overlay_webview(mtm)?;
        unsafe {
            let _: () = msg_send![&*webview, stopLoading];
        }
        Ok(())
    })?;
    emit_loading(&app, false);
    Ok(())
}

#[tauri::command]
pub async fn human_browser_query_nav_state(_app: AppHandle) -> Result<HumanBrowserNavStatePayload, String> {
    run_on_main(|mtm| {
        let webview = ensure_overlay_webview(mtm)?;
        let (can_go_back, can_go_forward) = read_nav_state(&webview);
        Ok(HumanBrowserNavStatePayload {
            can_go_back,
            can_go_forward,
        })
    })
}

#[tauri::command]
pub async fn human_browser_find_in_page(
    _app: AppHandle,
    query: String,
    forward: Option<bool>,
) -> Result<FindInPageResult, String> {
    let trimmed = query.trim().to_string();
    if trimmed.is_empty() {
        return Ok(FindInPageResult { found: false });
    }
    let forward = forward.unwrap_or(true);
    let query_json = serde_json::to_string(&trimmed).map_err(|err| err.to_string())?;
    let backwards = if forward { "false" } else { "true" };
    let script = format!(
        "(function(){{try{{return window.find({query_json},false,{backwards},true,false,true,false)?'true':'false';}}catch(e){{return 'false';}}}})();"
    );
    run_on_main(move |mtm| {
        let raw = eval_js_sync(mtm, &script)?;
        Ok(FindInPageResult {
            found: raw.trim() == "true",
        })
    })
}

#[tauri::command]
pub fn human_browser_report_find_result(_found: bool) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn human_browser_report_snapshot(_payload: SnapshotReport) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn human_browser_snapshot_html(app: AppHandle) -> Result<SnapshotHtmlDto, String> {
    let report = capture_html_async(&app).await?;
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
    let report = capture_html_async(&app).await?;
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
    let report = capture_html_async(&app).await?;
    Ok(search_response_from_ddg_html(
        &report.html,
        &report.url,
        &query,
        max_results.unwrap_or(8),
    ))
}
