//! The sole IPC surface exposed to arbitrary browser content webviews.
//!
//! This plugin can only report bounded results for requests already admitted by
//! trusted native code. The handler derives caller identity from `Webview` and
//! never accepts a page-supplied surface, URL, origin, or generation.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Runtime, Webview};

use crate::human_browser::{accept_browser_page_report, BrowserPageReport};

#[tauri::command]
fn report<R: Runtime>(webview: Webview<R>, report: BrowserPageReport) -> Result<(), String> {
    accept_browser_page_report(webview, report)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("browser-bridge")
        .invoke_handler(tauri::generate_handler![report])
        .on_drop(|_| crate::human_browser::shutdown_browser_bridge())
        .build()
}
