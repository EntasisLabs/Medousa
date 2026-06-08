use tauri::AppHandle;
#[cfg(desktop)]
use tauri::Manager;

pub fn set_app_badge_count(app: &AppHandle, blocked_count: u32) -> Result<(), String> {
    let badge = if blocked_count > 0 {
        Some(blocked_count as i64)
    } else {
        None
    };

    #[cfg(desktop)]
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_badge_count(badge)
            .map_err(|err| err.to_string())?;
    }

    #[cfg(target_os = "ios")]
    set_ios_badge_count(app, badge)?;

    Ok(())
}

#[cfg(target_os = "ios")]
fn set_ios_badge_count(app: &AppHandle, count: Option<i64>) -> Result<(), String> {
    let badge = count.map_or(0, |value| value.clamp(0, i32::MAX as i64) as i32);
    app.run_on_main_thread(move || apply_ios_badge_count(badge))
        .map_err(|err| err.to_string())
}

#[cfg(target_os = "ios")]
fn apply_ios_badge_count(count: i32) {
    use std::ffi::CStr;

    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };

    // Same UIKit path Tauri/wry uses internally; WebviewWindow::set_badge_count is desktop-only.
    unsafe {
        let ui_application = AnyClass::get(CStr::from_bytes_with_nul(b"UIApplication\0").unwrap())
            .expect("Failed to get UIApplication class");
        let application: *mut AnyObject = msg_send![ui_application, sharedApplication];
        let _: () = msg_send![application, setApplicationIconBadgeNumber: count];
    }
}
