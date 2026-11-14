//! Canonical paths shared with the native desktop host.
pub use medousa_desktop_host::paths::*;
#[cfg(target_os = "android")]
use std::path::PathBuf;

#[cfg(target_os = "android")]
pub fn initialize_android_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;

    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("resolve Android app data directory: {error}"))?
        .join("medousa");
    std::fs::create_dir_all(&data_dir)
        .map_err(|error| format!("create Android app data directory: {error}"))?;
    medousa_desktop_host::paths::initialize_data_dir(data_dir)
}
