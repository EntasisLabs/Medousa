use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::spec::{build_autostart_spec, SERVICE_NAME, AutostartSpec};

fn launch_agents_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library")
        .join("LaunchAgents")
}

fn launch_agent_path() -> PathBuf {
    launch_agents_dir().join(format!("{SERVICE_NAME}.plist"))
}

fn launchctl_gui_domain() -> String {
    let uid = unsafe { libc::getuid() };
    format!("gui/{uid}")
}

pub fn install() -> Result<(), String> {
    let spec = build_autostart_spec()?;
    let log_path = spec.log_path.clone();

    if let Some(parent) = launch_agents_dir().parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::create_dir_all(launch_agents_dir()).map_err(|err| err.to_string())?;
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let plist = render_launch_agent_plist(&spec, &log_path);
    fs::write(launch_agent_path(), plist).map_err(|err| err.to_string())?;

    let domain = launchctl_gui_domain();
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
        return Err(
            "Could not register Medousa with launchd. Try again or start Medousa manually."
                .to_string(),
        );
    }
    Ok(())
}

pub fn remove() -> Result<(), String> {
    let path = launch_agent_path();
    if path.exists() {
        let domain = launchctl_gui_domain();
        let _ = Command::new("launchctl")
            .args(["bootout", &domain, &path.to_string_lossy()])
            .status();
        let _ = fs::remove_file(path);
    }
    Ok(())
}

fn render_launch_agent_plist(spec: &AutostartSpec, log_path: &PathBuf) -> String {
    let mut program_args = vec![spec.program.clone()];
    program_args.extend(spec.args.clone());
    let args_xml: String = program_args
        .iter()
        .map(|arg| format!("        <string>{}</string>", xml_escape(arg)))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{SERVICE_NAME}</string>
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
    )
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
