use std::path::PathBuf;

use crate::connection_prefs::load_connection_prefs;
use crate::local_engine::{
    local_brain_installed, resolve_backend, resolve_daemon_binary, resolve_local_binary,
    should_load_private_brain, DEFAULT_LOCAL_BIND, DEFAULT_LOCAL_BRAIN_BIND,
};

pub const SERVICE_NAME: &str = "medousa-engine";

pub struct AutostartSpec {
    pub program: String,
    pub args: Vec<String>,
    pub log_path: PathBuf,
    pub local_brain: Option<LocalBrainAutostart>,
}

pub struct LocalBrainAutostart {
    pub program: String,
    pub args: Vec<String>,
}

pub fn build_autostart_spec() -> Result<AutostartSpec, String> {
    let daemon = resolve_daemon_binary()?;
    let backend = resolve_backend();
    let private_brain = should_load_private_brain(false);
    let prefs = load_connection_prefs();
    let bind = if prefs.public_bind {
        "0.0.0.0:7419"
    } else {
        DEFAULT_LOCAL_BIND
    };

    let mut args = vec![
        "--backend".to_string(),
        backend,
        "--bind".to_string(),
        bind.to_string(),
    ];
    args.extend(daemon.pre_args);

    let local_brain = if private_brain && local_brain_installed() {
        let local = resolve_local_binary()?;
        Some(LocalBrainAutostart {
            program: local.program,
            args: vec![
                "--bind".to_string(),
                DEFAULT_LOCAL_BRAIN_BIND.to_string(),
                "--load-recommended".to_string(),
            ],
        })
    } else {
        None
    };

    Ok(AutostartSpec {
        program: daemon.program,
        args,
        log_path: daemon_log_path(),
        local_brain,
    })
}

pub fn daemon_log_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("logs")
        .join("daemon.log")
}

pub fn local_brain_log_path() -> PathBuf {
    daemon_log_path()
        .with_file_name("local.log")
}

pub fn shell_start_command(spec: &AutostartSpec) -> String {
    let mut parts = vec![shell_quote(&spec.program)];
    parts.extend(spec.args.iter().map(|arg| shell_quote(arg)));
    let daemon = parts.join(" ");
    let mut script = format!(
        "{daemon} >> {} 2>&1",
        shell_quote(&spec.log_path.display().to_string())
    );
    if let Some(local) = &spec.local_brain {
        let mut local_parts = vec![shell_quote(&local.program)];
        local_parts.extend(local.args.iter().map(|arg| shell_quote(arg)));
        script.push_str(" & ");
        script.push_str(&local_parts.join(" "));
        script.push_str(" >> ");
        script.push_str(&shell_quote(
            &local_brain_log_path().display().to_string(),
        ));
        script.push_str(" 2>&1");
    }
    script.push_str(" & wait");
    script
}

fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
