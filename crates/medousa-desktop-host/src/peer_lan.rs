//! Explicit LAN pairing window for the managed personal engine only.
//! This never changes pairing credentials or the private tunnel configuration.
use crate::workshop_registry::{
    self as registry, PERSONAL_WORKSHOP_ID, WorkshopRegistry, WorkshopServer,
};
use crate::workshop_runtime as runtime;
use serde_json::{Value, json};
use std::{fs, net::SocketAddr, time::Duration};

static CHANGE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Debug)]
struct Target {
    workshop: WorkshopServer,
    bind: String,
    port: u16,
}

// Read without ensure_migrated/load_registry: even a rejected request must not
// migrate or write somebody else's registry as a side effect.
fn read_registry() -> Result<WorkshopRegistry, String> {
    let raw = fs::read(registry::workshops_registry_path()).map_err(|e| e.to_string())?;
    let value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    registry::validate_registry(&value)?;
    Ok(value)
}

fn target(value: &WorkshopRegistry, local_url: &str) -> Result<Target, String> {
    let mut personal = value
        .workshops
        .iter()
        .filter(|w| w.id == PERSONAL_WORKSHOP_ID);
    let workshop = personal.next().ok_or("Personal workshop not found")?;
    if personal.next().is_some() {
        return Err("Personal workshop registry identity is ambiguous".into());
    }
    if workshop.kind != "local" || workshop.pairing.is_some() {
        return Err("LAN settings require the managed personal workshop".into());
    }
    let bind = runtime::resolve_workshop_bind(workshop);
    let address: SocketAddr = bind.parse().map_err(|_| "Invalid personal engine bind")?;
    if address.port() == 0 || !(address.ip().is_loopback() || address.ip().is_unspecified()) {
        return Err("LAN settings do not manage custom interface binds".into());
    }
    let expected = runtime::url_from_bind(&bind);
    // Exact endpoint comparison intentionally does not accept localhost aliases,
    // credentials, paths, queries or ports belonging to another local daemon.
    if local_url != expected || workshop.url != expected {
        return Err("LAN settings target does not match the personal registry bind".into());
    }
    Ok(Target {
        workshop: workshop.clone(),
        bind,
        port: address.port(),
    })
}

fn response(bind: &str, restarted: bool) -> Value {
    let enabled = runtime::is_public_bind(bind);
    let message = match (enabled, restarted) {
        (true, true) => {
            "Engine restarted with LAN pairing on. Pair phones and peers, then turn this off."
        }
        (false, true) => {
            "Engine restarted on loopback only. Remote clients use the private tunnel."
        }
        (true, false) => {
            "LAN pairing is on — engine is reachable on your Wi-Fi. Turn off when done pairing."
        }
        (false, false) => {
            "LAN pairing is off — engine is loopback-only. Paired phones/peers keep working over the private tunnel."
        }
    };
    json!({ "enabled": enabled, "bind": bind, "url": runtime::url_from_bind(bind), "message": message })
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("LAN task failed: {e}"))?
}

/// Read the configured pairing window; unavailable for custom or remote targets.
pub async fn status(local_url: &str) -> Result<Value, String> {
    let local_url = local_url.to_owned();
    blocking(move || {
        let selected = target(&read_registry()?, &local_url)?;
        owned_pid(&selected)?;
        Ok(response(&selected.bind, false))
    })
    .await
}

/// Restart only a positively identified managed personal listener. No retries
/// after unknown outcomes, and no broad process-name/sidecar termination.
pub async fn set_enabled(local_url: &str, enabled: bool) -> Result<Value, String> {
    let _guard = CHANGE_LOCK.lock().await;
    let local_url = local_url.to_owned();
    let selected = blocking(move || target(&read_registry()?, &local_url)).await?;
    let check = selected.clone();
    let pid = blocking(move || owned_pid(&check)).await?;
    let next_bind = planned_bind(&selected, enabled);
    if next_bind == selected.bind {
        return Ok(response(&selected.bind, false));
    }
    let check = selected.clone();
    blocking(move || {
        // Recheck both the saved endpoint and listener immediately before signal.
        let current = target(&read_registry()?, &runtime::url_from_bind(&check.bind))?;
        if current.bind != check.bind
            || current.workshop.data_dir != check.workshop.data_dir
            || owned_pid(&current)? != pid
        {
            return Err("Personal engine changed while preparing LAN restart".into());
        }
        request_stop(pid)
    })
    .await?;
    let mut stopped = false;
    for _ in 0..40 {
        if !blocking(move || Ok(process_alive(pid))).await? {
            stopped = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    if !stopped {
        return Err(
            "Personal engine did not stop within 10 seconds; LAN settings were not changed".into(),
        );
    }
    let bind = next_bind.clone();
    blocking(move || {
        let mut value = read_registry()?;
        // Refuse a replacement listener rather than reusing a healthy unrelated daemon.
        if runtime::is_bind_reachable(&bind) {
            return Err("LAN restart target port is occupied; engine remains stopped".into());
        }
        update_registry(&mut value, &selected, &bind)?;
        let data_dir = runtime::resolve_workshop_data_dir(&selected.workshop);
        registry::save_registry(&value)?;
        runtime::spawn_local_engine(PERSONAL_WORKSHOP_ID, &bind, &data_dir, false)
            .map_err(|e| format!("LAN setting saved but engine restart failed: {e}"))?;
        Ok(())
    })
    .await?;
    let url = runtime::url_from_bind(&next_bind);
    if !runtime::wait_engine_healthy(&url, 45, 500).await?.0 {
        return Err(
            "LAN setting saved but restarted engine did not become healthy; check engine logs"
                .into(),
        );
    }
    #[cfg(feature = "legacy-client-network")]
    crate::workshop_transport::invalidate_all_route_caches();
    Ok(response(&next_bind, true))
}

// Pure update: callers must verify stop and port availability before saving.
// Only bind/url/timestamp change; pairing, tunnel and other workshops survive.
fn update_registry(
    value: &mut WorkshopRegistry,
    selected: &Target,
    bind: &str,
) -> Result<(), String> {
    let current = target(value, &runtime::url_from_bind(&selected.bind))?;
    if serde_json::to_value(&current.workshop).map_err(|e| e.to_string())?
        != serde_json::to_value(&selected.workshop).map_err(|e| e.to_string())?
    {
        return Err("Registry changed during LAN restart; engine remains stopped".into());
    }
    let workshop = value
        .workshops
        .iter_mut()
        .find(|w| w.id == PERSONAL_WORKSHOP_ID)
        .ok_or("Personal workshop not found")?;
    workshop.bind = Some(bind.to_owned());
    workshop.url = runtime::url_from_bind(bind);
    workshop.updated_at = registry::now_iso();
    Ok(())
}

fn planned_bind(selected: &Target, enabled: bool) -> String {
    // Match the desktop adapter: public IPv4 while pairing, loopback otherwise.
    format!(
        "{}:{}",
        if enabled { "0.0.0.0" } else { "127.0.0.1" },
        selected.port
    )
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn process_alive(pid: u32) -> bool {
    if !medousa_host::is_process_alive(pid) {
        return false;
    }
    // spawn_local_engine drops its Child handle; a terminated child may remain
    // a zombie until the desktop process reaps it. It no longer owns sockets.
    #[cfg(unix)]
    if let Ok(output) = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat="])
        .output()
    {
        if output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .trim()
                .starts_with('Z')
        {
            return false;
        }
    }
    true
}
#[cfg(any(target_os = "ios", target_os = "android"))]
fn process_alive(_pid: u32) -> bool {
    false
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn request_stop(pid: u32) -> Result<(), String> {
    if medousa_host::request_process_stop_by_pid(pid) {
        Ok(())
    } else {
        Err("Failed to stop managed personal engine; LAN settings were not changed".into())
    }
}
#[cfg(any(target_os = "ios", target_os = "android"))]
fn request_stop(_pid: u32) -> Result<(), String> {
    Err("LAN restart requires a desktop managed engine".into())
}

fn owned_pid(selected: &Target) -> Result<u32, String> {
    let primary = runtime::daemon_pid_path(PERSONAL_WORKSHOP_ID);
    let legacy = crate::paths::medousa_data_dir().join("daemon.pid");
    let raw = match fs::read_to_string(primary) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::read_to_string(legacy)
            .map_err(|_| "Personal engine is not tracked by the managed runtime")?,
        Err(e) => return Err(e.to_string()),
    };
    let pid: u32 = raw
        .trim()
        .parse()
        .map_err(|_| "Invalid managed engine PID")?;
    if pid == 0 || pid > i32::MAX as u32 || !process_alive(pid) {
        return Err("Managed personal engine is not running".into());
    }
    verify_listener(pid, selected)?;
    Ok(pid)
}

// A PID file alone is insufficient (it may be stale/reused). Require both the
// managed executable + exact bind argument and ownership of the listening port.
#[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
fn verify_listener(pid: u32, selected: &Target) -> Result<(), String> {
    use std::process::Command;
    let daemon = runtime::resolve_daemon_binary()?;
    let output = Command::new("ps")
        .args(["-ww", "-p", &pid.to_string(), "-o", "command="])
        .output()
        .map_err(|e| e.to_string())?;
    let command = String::from_utf8_lossy(&output.stdout);
    let mut expected = daemon.program;
    for arg in daemon.pre_args {
        expected.push(' ');
        expected.push_str(&arg);
    }
    if !output.status.success() || !command_matches(command.trim(), &expected, &selected.bind) {
        return Err("Tracked PID is not the managed personal engine for this bind".into());
    }
    let output = Command::new("lsof")
        .args([
            "-nP",
            "-a",
            "-p",
            &pid.to_string(),
            &format!("-iTCP:{}", selected.port),
            "-sTCP:LISTEN",
            "-t",
        ])
        .output()
        .map_err(|e| format!("Cannot verify personal listener ownership (lsof): {e}"))?;
    if !output.status.success()
        || !String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.trim() == pid.to_string())
    {
        return Err("Tracked managed PID does not own the personal listening port".into());
    }
    Ok(())
}

#[cfg(windows)]
fn verify_listener(pid: u32, selected: &Target) -> Result<(), String> {
    let daemon = runtime::resolve_daemon_binary()?;
    // Only typed numeric PID/port are interpolated, never URL or shell input.
    let script = format!(
        "$ErrorActionPreference='Stop'; $p=Get-CimInstance Win32_Process -Filter 'ProcessId={pid}'; \
         $owners=@(Get-NetTCPConnection -LocalPort {} -State Listen | Select-Object -ExpandProperty OwningProcess); \
         @{{executable=$p.ExecutablePath; command=$p.CommandLine; owns=($owners -contains {pid})}} | ConvertTo-Json -Compress",
        selected.port
    );
    let mut command = std::process::Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    medousa_host::hide_subprocess_window(&mut command);
    let output = command
        .output()
        .map_err(|e| format!("Cannot verify personal listener ownership: {e}"))?;
    if !output.status.success() {
        return Err("Cannot verify managed personal process and listener ownership".into());
    }
    let info: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
    let actual = info["executable"]
        .as_str()
        .ok_or("Missing managed process executable")?;
    let expected = std::path::Path::new(&daemon.program)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let actual = std::path::Path::new(actual)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let mut prefix = format!("\"{}\"", daemon.program);
    for arg in daemon.pre_args {
        prefix.push(' ');
        prefix.push_str(&arg);
    }
    let running = info["command"].as_str().unwrap_or_default();
    if actual != expected
        || info["owns"] != true
        || !command_matches(running, &prefix, &selected.bind)
    {
        return Err("Tracked PID is not the managed personal listener for this bind".into());
    }
    Ok(())
}

#[cfg(not(any(windows, all(unix, not(any(target_os = "ios", target_os = "android"))))))]
fn verify_listener(_pid: u32, _selected: &Target) -> Result<(), String> {
    Err("Managed LAN restart ownership verification is unavailable on this platform".into())
}

fn command_matches(command: &str, expected: &str, bind: &str) -> bool {
    let Some(rest) = command
        .strip_prefix(expected)
        .filter(|rest| rest.starts_with(' '))
    else {
        return false;
    };
    let args: Vec<_> = rest.split_whitespace().collect();
    let binds: Vec<_> = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| **arg == "--bind" || arg.starts_with("--bind="))
        .collect();
    binds.len() == 1 && *binds[0].1 == "--bind" && args.get(binds[0].0 + 1) == Some(&bind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_preserve_port_and_private_tunnel_fields() {
        let mut value = registry::default_registry();
        value.workshops[0].bind = Some("127.0.0.1:7491".into());
        value.workshops[0].url = "http://127.0.0.1:7491".into();
        let before = serde_json::to_value(&value).unwrap();
        let selected = target(&value, "http://127.0.0.1:7491").unwrap();
        assert_eq!(planned_bind(&selected, true), "0.0.0.0:7491");
        assert_eq!(planned_bind(&selected, false), "127.0.0.1:7491");
        assert_eq!(serde_json::to_value(value).unwrap(), before);
    }

    #[test]
    fn rejects_remote_custom_alias_and_mismatched_registry_urls() {
        let value = registry::default_registry();
        for url in [
            "http://localhost:7419",
            "http://127.0.0.1:7420",
            "http://127.0.0.1:7419/",
            "http://other:7419",
            "http://127.0.0.1:7419?x=1",
        ] {
            assert!(target(&value, url).is_err(), "{url}");
        }
        let mut value = value;
        value.workshops[0].url = "http://custom:7419".into();
        assert!(target(&value, "http://127.0.0.1:7419").is_err());
        value.workshops[0].kind = "portal".into();
        assert!(target(&value, "http://custom:7419").is_err());
    }

    #[test]
    fn rejects_custom_interface_invalid_bind_and_nonpersonal() {
        let mut value = registry::default_registry();
        for bind in [
            "192.168.1.2:7419",
            "127.0.0.1:0",
            "localhost:7419",
            "garbage",
        ] {
            value.workshops[0].bind = Some(bind.into());
            value.workshops[0].url = runtime::url_from_bind(bind);
            assert!(target(&value, &value.workshops[0].url).is_err());
        }
        value.workshops[0].id = "another-local".into();
        assert!(target(&value, "http://127.0.0.1:7419").is_err());
    }

    #[test]
    fn registry_update_preserves_every_field_except_bind_url_timestamp() {
        let mut value = registry::default_registry();
        value.workshops[0].data_dir = Some("/isolated/personal".into());
        let mut other = value.workshops[0].clone();
        other.id = "another-local".into();
        value.workshops.push(other);
        let selected = target(&value, "http://127.0.0.1:7419").unwrap();
        let mut expected = serde_json::to_value(&value).unwrap();
        update_registry(&mut value, &selected, &planned_bind(&selected, true)).unwrap();
        expected["workshops"][0]["bind"] = json!("0.0.0.0:7419");
        expected["workshops"][0]["url"] = json!("http://127.0.0.1:7419");
        expected["workshops"][0]["updatedAt"] = json!(value.workshops[0].updated_at);
        assert_eq!(serde_json::to_value(value).unwrap(), expected);
    }

    #[test]
    fn stale_registry_plan_and_duplicate_personal_identity_are_rejected_without_mutation() {
        let mut value = registry::default_registry();
        let selected = target(&value, "http://127.0.0.1:7419").unwrap();
        value.workshops[0].data_dir = Some("/changed".into());
        let before = serde_json::to_value(&value).unwrap();
        assert!(update_registry(&mut value, &selected, "0.0.0.0:7419").is_err());
        assert_eq!(serde_json::to_value(&value).unwrap(), before);
        value.workshops.push(value.workshops[0].clone());
        assert!(target(&value, "http://127.0.0.1:7419").is_err());
    }

    #[test]
    fn public_status_retains_loopback_client_url_and_tunnel_message() {
        let on = response("0.0.0.0:7491", true);
        assert_eq!(on["enabled"], true);
        assert_eq!(on["url"], "http://127.0.0.1:7491");
        let off = response("127.0.0.1:7491", true);
        assert_eq!(off["enabled"], false);
        assert!(off["message"].as_str().unwrap().contains("private tunnel"));
    }

    #[test]
    fn reused_pid_or_wrong_bind_commands_are_rejected() {
        assert!(command_matches(
            "/bin/medousa_daemon --backend surreal-mem --bind 0.0.0.0:7419",
            "/bin/medousa_daemon",
            "0.0.0.0:7419"
        ));
        for command in [
            "/bin/medousa_daemon-other --bind 127.0.0.1:7419",
            "/bin/other --bind 127.0.0.1:7419",
            "/bin/medousa_daemon --bind 127.0.0.1:7420",
            "/bin/medousa_daemon --binding 127.0.0.1:7419",
            "/bin/medousa_daemon --bind 127.0.0.1:7419 --bind 0.0.0.0:7419",
            "/bin/medousa_daemon --bind 127.0.0.1:7419 --bind=0.0.0.0:7419",
        ] {
            assert!(!command_matches(
                command,
                "/bin/medousa_daemon",
                "127.0.0.1:7419"
            ));
        }
    }
}
