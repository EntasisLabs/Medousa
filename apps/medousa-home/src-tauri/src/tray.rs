use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn tray_update_blocked_count(app: AppHandle, blocked_count: u32) -> Result<(), String> {
    let tooltip = if blocked_count > 0 {
        format!("Medousa Home · {blocked_count} blocked")
    } else {
        "Medousa Home".to_string()
    };

    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_tooltip(Some(&tooltip))
            .map_err(|err| err.to_string())?;

        #[cfg(target_os = "linux")]
        {
            let title = if blocked_count > 0 {
                Some(blocked_count.to_string())
            } else {
                None
            };
            tray.set_title(title.as_deref())
                .map_err(|err| err.to_string())?;
        }
    }

    if let Some(window) = app.get_webview_window("main") {
        let badge = if blocked_count > 0 {
            Some(blocked_count as i64)
        } else {
            None
        };
        let _ = window.set_badge_count(badge);
    }

    Ok(())
}
