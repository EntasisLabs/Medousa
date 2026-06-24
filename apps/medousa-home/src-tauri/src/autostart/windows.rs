use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::spec::{build_autostart_spec, AutostartSpec};

const TASK_FOLDER: &str = "Medousa";
const TASK_NAME: &str = "medousa-engine";

fn task_full_name() -> String {
    format!("{TASK_FOLDER}\\{TASK_NAME}")
}

fn autostart_script_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("bin")
        .join("start-engine.cmd")
}

pub fn install() -> Result<(), String> {
    let spec = build_autostart_spec()?;
    let log_path = spec.log_path.clone();
    let script_path = autostart_script_path();

    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    if let Some(parent) = script_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    fs::write(&script_path, render_start_script(&spec, &log_path))
        .map_err(|err| err.to_string())?;

    // Remove any previous registration before creating — prefs may have changed.
    let _ = run_schtasks(&["/Delete", "/TN", &task_full_name(), "/F"]);

    run_schtasks(&[
        "/Create",
        "/TN",
        &task_full_name(),
        "/TR",
        &script_path.to_string_lossy(),
        "/SC",
        "ONLOGON",
        "/RL",
        "LIMITED",
        "/F",
    ])
}

pub fn remove() -> Result<(), String> {
    let _ = run_schtasks(&["/Delete", "/TN", &task_full_name(), "/F"]);
    let _ = fs::remove_file(autostart_script_path());
    Ok(())
}

fn render_start_script(spec: &AutostartSpec, log_path: &PathBuf) -> String {
    let work_dir = Path::new(&spec.program)
        .parent()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| ".".to_string());
    let args = spec
        .args
        .iter()
        .map(|arg| batch_quote(arg))
        .collect::<Vec<_>>()
        .join(" ");

    let mut script = format!(
        "@echo off\r\n\
         cd /d {}\r\n\
         start \"\" /B {} {} >> {} 2>&1\r\n",
        batch_quote(&work_dir),
        batch_quote(&spec.program),
        args,
        batch_quote(&log_path.to_string_lossy()),
    );

    if let Some(local) = &spec.local_brain {
        let local_args = local
            .args
            .iter()
            .map(|arg| batch_quote(arg))
            .collect::<Vec<_>>()
            .join(" ");
        let local_log = super::spec::local_brain_log_path();
        script.push_str(&format!(
            "start \"\" /B {} {} >> {} 2>&1\r\n",
            batch_quote(&local.program),
            local_args,
            batch_quote(&local_log.display().to_string()),
        ));
    }

    script
}

fn batch_quote(value: &str) -> String {
    if value.contains(' ') || value.contains('\t') || value.contains('"') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn run_schtasks(args: &[&str]) -> Result<(), String> {
    let output = Command::new("schtasks")
        .args(args)
        .output()
        .map_err(|err| format!("schtasks failed to start: {err}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stderr} ({stdout})")
    };
    Err(format!(
        "Could not register Medousa with Task Scheduler (schtasks {}): {detail}",
        args.join(" ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_script_quotes_paths_with_spaces() {
        let spec = AutostartSpec {
            program: r"C:\Program Files\Medousa\medousa_daemon.exe".to_string(),
            args: vec![
                "--backend".to_string(),
                "surreal-mem".to_string(),
                "--bind".to_string(),
                "127.0.0.1:7419".to_string(),
            ],
            log_path: PathBuf::from(r"C:\Users\me\AppData\Local\medousa\logs\daemon.log"),
        };

        let script = render_start_script(&spec, &spec.log_path);
        assert!(script.contains(r#"cd /d "C:\Program Files\Medousa""#));
        assert!(script.contains(r#"medousa_daemon.exe" --backend surreal-mem --bind 127.0.0.1:7419 >>"#));
    }
}
