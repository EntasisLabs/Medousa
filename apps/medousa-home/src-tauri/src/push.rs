use std::sync::Mutex;

static APNS_DEVICE_TOKEN: Mutex<Option<String>> = Mutex::new(None);

pub fn set_apns_device_token(token: Option<String>) {
    let trimmed = token
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Ok(mut guard) = APNS_DEVICE_TOKEN.lock() {
        *guard = trimmed;
    }
}

pub fn current_apns_device_token() -> Option<String> {
    APNS_DEVICE_TOKEN
        .lock()
        .ok()
        .and_then(|guard| guard.clone())
}

#[tauri::command]
pub fn push_register_apns_token(token: String) -> Result<(), String> {
    set_apns_device_token(Some(token));
    Ok(())
}

#[tauri::command]
pub fn push_clear_apns_token() -> Result<(), String> {
    set_apns_device_token(None);
    Ok(())
}
