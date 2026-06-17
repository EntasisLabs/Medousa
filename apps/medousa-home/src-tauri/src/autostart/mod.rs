//! Install the Medousa daemon to start at login.
//!
//! Each platform uses its native session manager — not interchangeable Unix APIs:
//! - macOS: launchd LaunchAgent + `launchctl`
//! - Linux: systemd user unit + `systemctl --user`
//! - Windows: not implemented yet (planned: Task Scheduler or Run key)

mod spec;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod stub;

pub fn autostart_supported() -> bool {
    cfg!(any(target_os = "macos", target_os = "linux"))
}

#[cfg(target_os = "macos")]
pub fn install_autostart() -> Result<(), String> {
    macos::install()
}

#[cfg(target_os = "linux")]
pub fn install_autostart() -> Result<(), String> {
    linux::install()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn install_autostart() -> Result<(), String> {
    stub::install()
}

#[cfg(target_os = "macos")]
pub fn remove_autostart() -> Result<(), String> {
    macos::remove()
}

#[cfg(target_os = "linux")]
pub fn remove_autostart() -> Result<(), String> {
    linux::remove()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn remove_autostart() -> Result<(), String> {
    stub::remove()
}
