use tauri::AppHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use tauri::Manager;

use crate::badge;

#[tauri::command]
pub fn tray_update_blocked_count(app: AppHandle, blocked_count: u32) -> Result<(), String> {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    {
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
    }

    badge::set_app_badge_count(&app, blocked_count)?;

    Ok(())
}
