use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::connection_prefs::load_connection_prefs;
use crate::daemon_service::{resolve_backend, resolve_daemon_binary, should_load_private_brain};

const LAUNCH_AGENT_LABEL: &str = "com.medousa.engine";

pub fn autostart_supported() -> bool {
    cfg!(target_os = "macos")
}

fn launch_agents_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library")
        .join("LaunchAgents")
}

fn launch_agent_path() -> PathBuf {
    launch_agents_dir().join(format!("{LAUNCH_AGENT_LABEL}.plist"))
}

fn daemon_log_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("logs")
        .join("daemon.log")
}

pub fn install_autostart() -> Result<(), String> {
    if !autostart_supported() {
        return Err("Auto-start is only available on macOS for now.".to_string());
    }

    let daemon = resolve_daemon_binary()?;
    let backend = resolve_backend();
    let private_brain = should_load_private_brain(false);
    let prefs = load_connection_prefs();
    let bind = if prefs.public_bind {
        "0.0.0.0:7419"
    } else {
        "127.0.0.1:7419"
    };
    let log_path = daemon_log_path();

    if let Some(parent) = launch_agents_dir().parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::create_dir_all(launch_agents_dir()).map_err(|err| err.to_string())?;
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let mut args = vec![
        daemon.program,
        "--backend".to_string(),
        backend,
        "--bind".to_string(),
        bind.to_string(),
    ];
    args.extend(daemon.pre_args);
    if private_brain {
        args.push("--local-engine".to_string());
    }
    if prefs.public_bind {
        // Pairing + LAN stream URLs when bound publicly.
        // LaunchAgent has no shell — set via ProgramArguments env is plist StandardErrorPath only;
        // daemon detects 0.0.0.0 bind for mDNS.
    }

    let args_xml: String = args
        .iter()
        .map(|arg| format!("        <string>{}</string>", xml_escape(arg)))
        .collect::<Vec<_>>()
        .join("\n");

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCH_AGENT_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
{args_xml}
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <false/>
  <key>StandardOutPath</key>
  <string>{}</string>
  <key>StandardErrorPath</key>
  <string>{}</string>
</dict>
</plist>
"#,
        xml_escape(&log_path.display().to_string()),
        xml_escape(&log_path.display().to_string()),
    );

    fs::write(launch_agent_path(), plist).map_err(|err| err.to_string())?;

    let uid = unsafe { libc::getuid() };
    let domain = format!("gui/{uid}");
    let path = launch_agent_path();
    let path_str = path.to_string_lossy();
    let _ = Command::new("launchctl")
        .args(["bootout", &domain, &path_str])
        .status();
    let status = Command::new("launchctl")
        .args(["bootstrap", &domain, &path_str])
        .status()
        .map_err(|err| format!("launchctl bootstrap failed: {err}"))?;
    if !status.success() {
        return Err("Could not register Medousa with launchd. Try again or start Medousa manually.".to_string());
    }
    Ok(())
}

pub fn remove_autostart() -> Result<(), String> {
    if !autostart_supported() {
        return Ok(());
    }

    let path = launch_agent_path();
    if path.exists() {
        let uid = unsafe { libc::getuid() };
        let _ = Command::new("launchctl")
            .args([
                "bootout",
                &format!("gui/{uid}"),
                &path.to_string_lossy(),
            ])
            .status();
        let _ = fs::remove_file(path);
    }
    Ok(())
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
