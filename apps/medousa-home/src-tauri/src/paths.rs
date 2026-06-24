//! Canonical Medousa storage paths — keep in sync with `Medousa/src/paths.rs`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static RESOLVED_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub const DATA_DIR_REDIRECT_FILENAME: &str = "data_dir";

pub fn default_medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

pub fn data_dir_redirect_path() -> PathBuf {
    default_medousa_data_dir().join(DATA_DIR_REDIRECT_FILENAME)
}

fn read_data_dir_redirect() -> Option<PathBuf> {
    let path = data_dir_redirect_path();
    let raw = std::fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

fn data_dir_from_env() -> Option<PathBuf> {
    std::env::var("MEDOUSA_DATA_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn resolve_medousa_data_dir() -> PathBuf {
    if let Some(from_env) = data_dir_from_env() {
        return from_env;
    }
    if let Some(from_redirect) = read_data_dir_redirect() {
        return from_redirect;
    }
    default_medousa_data_dir()
}

pub fn medousa_data_dir() -> PathBuf {
    RESOLVED_DATA_DIR
        .get_or_init(resolve_medousa_data_dir)
        .clone()
}

pub fn medousa_config_dir() -> PathBuf {
    std::env::var("MEDOUSA_CONFIG_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("medousa")
        })
}

pub fn medousa_data_dir_source() -> &'static str {
    if data_dir_from_env().is_some() {
        "MEDOUSA_DATA_DIR"
    } else if read_data_dir_redirect().is_some() {
        "bootstrap_redirect"
    } else {
        "default"
    }
}

pub fn set_data_dir_redirect(target: &Path) -> std::io::Result<()> {
    let redirect = data_dir_redirect_path();
    if let Some(parent) = redirect.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(redirect, format!("{}\n", target.display()))?;
    Ok(())
}
