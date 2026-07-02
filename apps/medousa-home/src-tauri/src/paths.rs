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

/// Load `.env` from Medousa config/data dirs without overriding existing vars.
/// Matches `medousa_daemon` overlay resolution (first existing file wins).
pub fn load_env_overlay() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(explicit) = std::env::var("STASIS_ENV_FILE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            candidates.push(PathBuf::from(trimmed));
        }
    }
    candidates.push(medousa_config_dir().join(".env"));
    candidates.push(medousa_data_dir().join(".env"));

    for path in candidates {
        if !path.is_file() {
            continue;
        }
        if let Err(err) = apply_env_file(&path) {
            eprintln!(
                "[medousa-home] failed to load env file {}: {err}",
                path.display()
            );
            continue;
        }
        return Some(path);
    }
    None
}

fn apply_env_file(path: &Path) -> std::io::Result<()> {
    let raw = std::fs::read_to_string(path)?;
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || std::env::var_os(key).is_some() {
            continue;
        }
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        // SAFETY: called once at process startup before spawning children.
        unsafe { std::env::set_var(key, value) };
    }
    Ok(())
}
