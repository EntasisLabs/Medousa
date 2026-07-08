//! Data-dir resolution mirroring the main `medousa` crate's `paths` module so the
//! offline brain writes the hardware profile to the same location the app uses.
//! Kept dependency-light (only `dirs` + std) to preserve crate leanness.

use std::path::PathBuf;
use std::sync::OnceLock;

static RESOLVED_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

const DATA_DIR_REDIRECT_FILENAME: &str = "data_dir";

fn default_medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

fn data_dir_redirect_path() -> PathBuf {
    default_medousa_data_dir().join(DATA_DIR_REDIRECT_FILENAME)
}

fn read_data_dir_redirect() -> Option<PathBuf> {
    let raw = std::fs::read_to_string(data_dir_redirect_path()).ok()?;
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

fn resolve_medousa_data_dir() -> PathBuf {
    if let Some(from_env) = data_dir_from_env() {
        return from_env;
    }
    if let Some(from_redirect) = read_data_dir_redirect() {
        return from_redirect;
    }
    default_medousa_data_dir()
}

/// Resolved engine/client data directory for this process.
pub fn medousa_data_dir() -> PathBuf {
    RESOLVED_DATA_DIR
        .get_or_init(resolve_medousa_data_dir)
        .clone()
}
