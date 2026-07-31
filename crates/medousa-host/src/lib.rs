//! Host-side process utilities shared by CLI and desktop app.

mod spawn;

pub use spawn::{
    medousa_local_binary_available, spawn_and_wait, spawn_and_wait_recommended,
    spawn_medousa_local, spawn_medousa_local_recommended, wait_local_engine_ready,
};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Workshop services spawned lazily by `medousa_daemon` but listening outside
/// its HTTP process. They must be replaced with the daemon during upgrades so
/// an old API revision cannot survive an Engine restart.
pub const WORKSHOP_SIDECAR_PROCESS_NAMES: &[&str] = &["medousa-code", "medousa-session"];

fn run_process_control(command: &mut Command) -> bool {
    command.stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    detach_new_session(command);
    command
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn request_process_stop_by_name(name: &str) -> bool {
    #[cfg(unix)]
    {
        run_process_control(Command::new("pkill").args(["-x", name]))
    }
    #[cfg(windows)]
    {
        let image = if name.ends_with(".exe") {
            name.to_string()
        } else {
            format!("{name}.exe")
        };
        return run_process_control(Command::new("taskkill").args(["/F", "/IM", &image]));
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = name;
        false
    }
}

pub fn request_process_stop_by_pid(pid: u32) -> bool {
    #[cfg(unix)]
    {
        run_process_control(Command::new("kill").arg(pid.to_string()))
    }
    #[cfg(windows)]
    {
        return run_process_control(Command::new("taskkill").args([
            "/F",
            "/PID",
            &pid.to_string(),
        ]));
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

pub fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        run_process_control(Command::new("kill").args(["-0", &pid.to_string()]))
    }
    #[cfg(windows)]
    {
        let mut command = Command::new("tasklist");
        command.args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]);
        detach_new_session(&mut command);
        return command
            .output()
            .ok()
            .filter(|output| output.status.success())
            .is_some_and(|output| {
                String::from_utf8_lossy(&output.stdout).contains(&format!("\",\"{pid}\","))
            });
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

pub fn stop_workshop_sidecars() {
    for name in WORKSHOP_SIDECAR_PROCESS_NAMES {
        let _ = request_process_stop_by_name(name);
    }
}

pub fn is_bind_reachable(bind: &str) -> bool {
    if let Ok(mut addrs) = bind.to_socket_addrs()
        && let Some(addr) = addrs.next()
    {
        return TcpStream::connect_timeout(&addr, Duration::from_millis(250)).is_ok();
    }
    false
}

pub fn find_command_in_path(command: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file())
}

pub fn resolve_sibling_binary(name: &str) -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let sibling = current_exe.with_file_name(name);
    if sibling.is_file() {
        Some(sibling)
    } else {
        None
    }
}

pub fn resolve_medousa_local_binary() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("MEDOUSA_MEDOUSA_LOCAL_BIN") {
        let path = PathBuf::from(explicit.trim());
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "MEDOUSA_MEDOUSA_LOCAL_BIN points to missing file: {}",
            path.display()
        ));
    }

    let name = if cfg!(windows) {
        "medousa_local.exe"
    } else {
        "medousa_local"
    };

    if let Some(path) = resolve_sibling_binary(name) {
        return Ok(path);
    }

    if let Some(path) = find_command_in_path(name) {
        return Ok(path);
    }

    Err(
        "medousa_local binary not found — install the Offline brain package or set MEDOUSA_MEDOUSA_LOCAL_BIN"
            .to_string(),
    )
}

pub fn resolve_medousa_daemon_binary() -> Result<PathBuf, String> {
    if let Ok(explicit) = std::env::var("MEDOUSA_MEDOUSA_DAEMON_BIN") {
        let path = PathBuf::from(explicit.trim());
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "MEDOUSA_MEDOUSA_DAEMON_BIN points to missing file: {}",
            path.display()
        ));
    }

    let name = if cfg!(windows) {
        "medousa_daemon.exe"
    } else {
        "medousa_daemon"
    };

    if let Some(path) = resolve_sibling_binary(name) {
        return Ok(path);
    }

    if let Some(path) = find_command_in_path(name) {
        return Ok(path);
    }

    Err(
        "medousa_daemon binary not found — set MEDOUSA_MEDOUSA_DAEMON_BIN for development"
            .to_string(),
    )
}

#[cfg(unix)]
pub fn detach_new_session(command: &mut Command) {
    use std::io;
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Hide the console window when spawning console-subsystem children on Windows.
/// Without this, Home's daemon/brain/gateway spawns flash a cmd window that
/// kills the process if the user closes it.
#[cfg(windows)]
pub fn detach_new_session(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(any(unix, windows)))]
pub fn detach_new_session(_command: &mut Command) {}

/// Prevent a captured/background subprocess from creating a visible console
/// window on Windows. Synchronous helpers such as Git should remain children
/// of the daemon, so this intentionally does not create a new Unix session.
#[cfg(windows)]
pub fn hide_subprocess_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
pub fn hide_subprocess_window(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::WORKSHOP_SIDECAR_PROCESS_NAMES;

    #[test]
    fn workshop_restart_targets_every_daemon_sidecar() {
        assert_eq!(
            WORKSHOP_SIDECAR_PROCESS_NAMES,
            &["medousa-code", "medousa-session"]
        );
    }
}
