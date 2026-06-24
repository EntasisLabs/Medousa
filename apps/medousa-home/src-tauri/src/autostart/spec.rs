use std::path::PathBuf;

use crate::connection_prefs::load_connection_prefs;
use crate::local_engine::{resolve_backend, resolve_daemon_binary, should_load_private_brain};

pub const SERVICE_NAME: &str = "medousa-engine";

pub struct AutostartSpec {
    pub program: String,
    pub args: Vec<String>,
    pub log_path: PathBuf,
}

pub fn build_autostart_spec() -> Result<AutostartSpec, String> {
    let daemon = resolve_daemon_binary()?;
    let backend = resolve_backend();
    let private_brain = should_load_private_brain(false);
    let prefs = load_connection_prefs();
    let bind = if prefs.public_bind {
        "0.0.0.0:7419"
    } else {
        "127.0.0.1:7419"
    };

    let mut args = vec![
        "--backend".to_string(),
        backend,
        "--bind".to_string(),
        bind.to_string(),
    ];
    args.extend(daemon.pre_args);
    if private_brain {
        args.push("--local-engine".to_string());
    }

    Ok(AutostartSpec {
        program: daemon.program,
        args,
        log_path: daemon_log_path(),
    })
}

pub fn daemon_log_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
        .join("logs")
        .join("daemon.log")
}
