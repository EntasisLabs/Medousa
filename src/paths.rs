//! Canonical Medousa storage paths (`MEDOUSA_DATA_DIR`, bootstrap redirect, config dir).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static RESOLVED_DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Filename under the default data dir that points at a custom engine storage root.
pub const DATA_DIR_REDIRECT_FILENAME: &str = "data_dir";

/// Default storage root when no env or redirect is set: `{data_local_dir}/medousa`.
pub fn default_medousa_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("medousa")
}

/// Bootstrap redirect path (always under the default location, not the resolved dir).
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

/// Resolve data dir without process-wide cache (for tests and doctor labels).
pub fn resolve_medousa_data_dir() -> PathBuf {
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

/// Config directory (`MEDOUSA_CONFIG_DIR` or `{config_dir}/medousa`).
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

/// Default user vault root: `{data_dir}/vault`.
pub fn user_vault_root() -> PathBuf {
    medousa_data_dir().join("vault")
}

/// Which resolution path produced the active data dir.
pub fn medousa_data_dir_source() -> &'static str {
    if data_dir_from_env().is_some() {
        "MEDOUSA_DATA_DIR"
    } else if read_data_dir_redirect().is_some() {
        "bootstrap_redirect"
    } else {
        "default"
    }
}

/// Write bootstrap redirect so Settings can relocate storage without shell env.
pub fn set_data_dir_redirect(target: &Path) -> std::io::Result<()> {
    let redirect = data_dir_redirect_path();
    if let Some(parent) = redirect.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(redirect, format!("{}\n", target.display()))?;
    Ok(())
}

/// Remove bootstrap redirect (revert to default on next process start).
pub fn clear_data_dir_redirect() -> std::io::Result<()> {
    let redirect = data_dir_redirect_path();
    if redirect.is_file() {
        std::fs::remove_file(redirect)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_data_dir_ends_with_medousa() {
        let path = default_medousa_data_dir();
        assert_eq!(path.file_name().and_then(|name| name.to_str()), Some("medousa"));
    }

    #[test]
    fn redirect_path_lives_under_default_root() {
        let redirect = data_dir_redirect_path();
        assert!(redirect.ends_with(DATA_DIR_REDIRECT_FILENAME));
        assert!(redirect.starts_with(default_medousa_data_dir()));
    }
}
