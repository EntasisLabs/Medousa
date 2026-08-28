//! Host-owned OAuth browser presentation.
//!
//! OAuth credentials never enter Medousa's controllable browser. iOS presents
//! a system Safari view inside the app; other hosts use their system browser.

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[cfg(target_os = "ios")]
use std::ffi::CStr;
#[cfg(target_os = "ios")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "ios")]
use objc2::rc::Retained;
#[cfg(target_os = "ios")]
use objc2::runtime::{AnyClass, AnyObject};
#[cfg(target_os = "ios")]
use objc2::{msg_send, MainThreadMarker};
#[cfg(target_os = "ios")]
use objc2_foundation::{NSString, NSURL};
#[cfg(target_os = "ios")]
use objc2_ui_kit::{UIApplication, UIViewController, UIWindow};

#[cfg(target_os = "ios")]
static PRESENTED: AtomicBool = AtomicBool::new(false);

const CALLBACK_PATH: &str = "/oauth/callback";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const MAX_REQUEST_BYTES: usize = 16 * 1024;

pub struct OAuthBrowserSession {
    listener: TcpListener,
    redirect_uri: String,
}

pub struct OAuthBrowserCallback {
    stream: TcpStream,
    url: String,
}

impl OAuthBrowserSession {
    pub async fn bind() -> Result<Self, String> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|error| format!("start OAuth callback listener: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("read OAuth callback address: {error}"))?
            .port();
        Ok(Self {
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}{CALLBACK_PATH}"),
        })
    }

    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    pub async fn authorize(
        self,
        app: &tauri::AppHandle,
        authorization_url: &str,
    ) -> Result<OAuthBrowserCallback, String> {
        open(app, authorization_url)?;
        let callback = wait_for_callback(self.listener, &self.redirect_uri).await;
        if callback.is_err() {
            dismiss(app);
        }
        callback
    }
}

impl OAuthBrowserCallback {
    pub fn url(&self) -> &str {
        &self.url
    }

    pub async fn finish(mut self, app: &tauri::AppHandle, success: bool) {
        write_browser_response(&mut self.stream, success).await;
        dismiss(app);
    }
}

async fn read_callback_request(stream: &mut TcpStream) -> Result<Option<String>, String> {
    let mut request = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    loop {
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| format!("read OAuth callback: {error}"))?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if request.len() >= MAX_REQUEST_BYTES {
            return Err("OAuth callback request was too large".to_string());
        }
    }

    let request = std::str::from_utf8(&request)
        .map_err(|_| "OAuth callback request was not valid HTTP".to_string())?;
    let Some(first_line) = request.lines().next() else {
        return Ok(None);
    };
    let mut parts = first_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if method != "GET"
        || !(target == CALLBACK_PATH || target.starts_with(&format!("{CALLBACK_PATH}?")))
    {
        return Ok(None);
    }
    Ok(Some(target.to_string()))
}

async fn wait_for_callback(
    listener: TcpListener,
    redirect_uri: &str,
) -> Result<OAuthBrowserCallback, String> {
    let redirect_uri = redirect_uri.to_string();
    let wait = async move {
        loop {
            let (mut stream, _) = listener
                .accept()
                .await
                .map_err(|error| format!("accept OAuth callback: {error}"))?;
            match read_callback_request(&mut stream).await? {
                Some(target) => {
                    let suffix = target.strip_prefix(CALLBACK_PATH).unwrap_or_default();
                    return Ok(OAuthBrowserCallback {
                        stream,
                        url: format!("{redirect_uri}{suffix}"),
                    });
                }
                None => write_browser_response(&mut stream, false).await,
            }
        }
    };
    tokio::time::timeout(CALLBACK_TIMEOUT, wait)
        .await
        .map_err(|_| "MCP sign-in timed out".to_string())?
}

async fn write_browser_response(stream: &mut TcpStream, success: bool) {
    let (status, heading, detail) = if success {
        (
            "200 OK",
            "Connected",
            "You can close this page and return to Medousa.",
        )
    } else {
        (
            "400 Bad Request",
            "Could not connect",
            "Return to Medousa and try signing in again.",
        )
    };
    let body = format!(
        "<!doctype html><meta name=\"viewport\" content=\"width=device-width\"><title>{heading}</title><style>body{{font:16px system-ui;background:#111;color:#eee;display:grid;place-content:center;min-height:90vh;margin:0}}main{{max-width:28rem;padding:2rem}}h1{{font-size:1.5rem}}</style><main><h1>{heading}</h1><p>{detail}</p></main>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

#[cfg(target_os = "ios")]
fn root_view_controller(mtm: MainThreadMarker) -> Result<Retained<UIViewController>, String> {
    let app = UIApplication::sharedApplication(mtm);
    let window: Retained<UIWindow> = app.keyWindow().ok_or_else(|| "no key window".to_string())?;
    window
        .rootViewController()
        .ok_or_else(|| "no root view controller".to_string())
}

#[cfg(target_os = "ios")]
pub fn open(app: &tauri::AppHandle, url: &str) -> Result<(), String> {
    let url = url.to_string();
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result = (|| {
            let mtm = MainThreadMarker::new()
                .ok_or_else(|| "OAuth browser requires the main thread".to_string())?;
            let root = root_view_controller(mtm)?;
            let class = AnyClass::get(
                CStr::from_bytes_with_nul(b"SFSafariViewController\0").expect("static class name"),
            )
            .ok_or_else(|| "system Safari view is unavailable".to_string())?;
            let ns_url = NSURL::URLWithString(&NSString::from_str(&url))
                .ok_or_else(|| "invalid OAuth authorization URL".to_string())?;
            unsafe {
                let allocated: *mut AnyObject = msg_send![class, alloc];
                let controller: *mut AnyObject = msg_send![allocated, initWithURL: &*ns_url];
                let controller = Retained::from_raw(controller)
                    .ok_or_else(|| "could not create system Safari view".to_string())?;
                let completion: *mut AnyObject = std::ptr::null_mut();
                let _: () = msg_send![&*root,
                    presentViewController: &*controller,
                    animated: true,
                    completion: completion
                ];
            }
            PRESENTED.store(true, Ordering::Release);
            Ok(())
        })();
        let _ = tx.send(result);
    })
    .map_err(|error| error.to_string())?;
    rx.recv()
        .map_err(|_| "OAuth browser presentation channel closed".to_string())?
}

#[cfg(not(target_os = "ios"))]
pub fn open(_app: &tauri::AppHandle, url: &str) -> Result<(), String> {
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|error| error.to_string())
}

#[cfg(target_os = "ios")]
pub fn dismiss(app: &tauri::AppHandle) {
    if !PRESENTED.swap(false, Ordering::AcqRel) {
        return;
    }
    let _ = app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Ok(root) = root_view_controller(mtm) else {
            return;
        };
        unsafe {
            let completion: *mut AnyObject = std::ptr::null_mut();
            let _: () = msg_send![&*root,
                dismissViewControllerAnimated: true,
                completion: completion
            ];
        }
    });
}

#[cfg(not(target_os = "ios"))]
pub fn dismiss(_app: &tauri::AppHandle) {}
