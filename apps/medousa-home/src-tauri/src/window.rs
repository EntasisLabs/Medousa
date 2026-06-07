use tauri::{AppHandle, Manager, WebviewWindow};

#[tauri::command]
pub fn window_show_chat_popout(app: AppHandle) -> Result<(), String> {
    let window = chat_popout_window(&app)?;
    window.show().map_err(|err| err.to_string())?;
    window.set_focus().map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn window_hide_chat_popout(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("chat-popout") {
        window.hide().map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn chat_popout_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("chat-popout")
        .ok_or_else(|| "chat popout window is not configured".to_string())
}
