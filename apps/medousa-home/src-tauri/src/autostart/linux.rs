use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::spec::{build_autostart_spec, SERVICE_NAME, AutostartSpec};

fn systemd_user_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("systemd")
        .join("user")
}

fn systemd_unit_path() -> PathBuf {
    systemd_user_dir().join(format!("{SERVICE_NAME}.service"))
}

pub fn install() -> Result<(), String> {
    let spec = build_autostart_spec()?;
    let log_path = spec.log_path.clone();

    fs::create_dir_all(systemd_user_dir()).map_err(|err| err.to_string())?;
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let unit = render_systemd_unit(&spec, &log_path);
    fs::write(systemd_unit_path(), unit).map_err(|err| err.to_string())?;

    run_systemctl(&["daemon-reload"])?;
    run_systemctl(&["enable", "--now", &format!("{SERVICE_NAME}.service")])?;
    Ok(())
}

pub fn remove() -> Result<(), String> {
    let unit_path = systemd_unit_path();
    if unit_path.exists() {
        let _ = run_systemctl(&["disable", "--now", &format!("{SERVICE_NAME}.service")]);
        let _ = fs::remove_file(unit_path);
        let _ = run_systemctl(&["daemon-reload"]);
    }
    Ok(())
}

fn run_systemctl(args: &[&str]) -> Result<(), String> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .map_err(|err| format!("systemctl --user failed: {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "systemctl --user {} failed (exit {})",
            args.join(" "),
            status.code().unwrap_or(-1)
        ))
    }
}

fn render_systemd_unit(spec: &AutostartSpec, log_path: &PathBuf) -> String {
    let exec_start = systemd_exec_start(&spec.program, &spec.args);
    let log = log_path.display();

    format!(
        r#"[Unit]
Description=Medousa Engine
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={exec_start}
StandardOutput=append:{log}
StandardError=append:{log}
Restart=on-failure

[Install]
WantedBy=default.target
"#
    )
}

fn systemd_exec_start(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(systemd_escape)
        .collect::<Vec<_>>()
        .join(" ")
}

fn systemd_escape(value: &str) -> String {
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '\\' | '"' | '$'))
    {
        format!(
            "\"{}\"",
            value.replace('\\', "\\\\").replace('"', "\\\"")
        )
    } else {
        value.to_string()
    }
}
