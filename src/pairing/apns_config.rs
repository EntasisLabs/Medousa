//! Load APNs credentials from env, keychain, or `{medousa_data_dir}/apns/config.json`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::paths::medousa_data_dir;

use super::apns::ApnsConfig;
use super::apns_keychain;

const CONFIG_FILENAME: &str = "config.json";
const DEFAULT_BUNDLE_ID: &str = "com.entasislabs.medousa-home";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApnsConfigSource {
    Environment,
    DataDirFile,
    DataDirKeychain,
    None,
}

impl ApnsConfigSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::DataDirFile => "data_dir_file",
            Self::DataDirKeychain => "data_dir_keychain",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyStorage {
    Keychain,
    File,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApnsConfigFile {
    team_id: String,
    key_id: String,
    #[serde(default)]
    key_file: Option<String>,
    #[serde(default)]
    key_storage: Option<String>,
    #[serde(default)]
    bundle_id: Option<String>,
    #[serde(default)]
    sandbox: Option<bool>,
}

/// `{medousa_data_dir}/apns` — e.g. `~/Library/Application Support/medousa/apns` on macOS.
pub fn apns_config_dir() -> PathBuf {
    medousa_data_dir().join("apns")
}

pub fn apns_config_file_path() -> PathBuf {
    apns_config_dir().join(CONFIG_FILENAME)
}

pub fn load_apns_config() -> (Option<ApnsConfig>, ApnsConfigSource) {
    if let Some(config) = load_from_env() {
        return (Some(config), ApnsConfigSource::Environment);
    }
    match load_from_data_dir() {
        Ok((config, source)) => (Some(config), source),
        Err(_) => (None, ApnsConfigSource::None),
    }
}

fn load_from_env() -> Option<ApnsConfig> {
    let team_id = env_trimmed("MEDOUSA_APNS_TEAM_ID")?;
    let key_id = env_trimmed("MEDOUSA_APNS_KEY_ID")?;
    let key_path = env_trimmed("MEDOUSA_APNS_KEY_PATH")?;
    let key_pem = fs::read_to_string(&key_path)
        .with_context(|| format!("read APNs key at {key_path}"))
        .ok()?;
    Some(ApnsConfig {
        team_id,
        key_id,
        key_pem,
        bundle_id: env_trimmed("MEDOUSA_APNS_BUNDLE_ID").unwrap_or_else(|| DEFAULT_BUNDLE_ID.to_string()),
        sandbox: parse_sandbox_env(),
    })
}

fn load_from_data_dir() -> Result<(ApnsConfig, ApnsConfigSource)> {
    let config_path = apns_config_file_path();
    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    let file: ApnsConfigFile =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", config_path.display()))?;

    let team_id = file.team_id.trim().to_string();
    let key_id = file.key_id.trim().to_string();
    if team_id.is_empty() || key_id.is_empty() {
        anyhow::bail!("apns config.json requires teamId and keyId");
    }

    let storage = resolve_key_storage(&file);
    let (key_pem, source) = match storage {
        KeyStorage::Keychain => {
            let pem = apns_keychain::load_apns_key_pem()
                .with_context(|| "read APNs key from keychain (keyStorage: keychain)")?;
            (pem, ApnsConfigSource::DataDirKeychain)
        }
        KeyStorage::File => {
            let key_path = resolve_key_path(&file)?;
            let pem = fs::read_to_string(&key_path)
                .with_context(|| format!("read APNs key at {}", key_path.display()))?;
            (pem, ApnsConfigSource::DataDirFile)
        }
    };

    Ok((
        ApnsConfig {
            team_id,
            key_id,
            key_pem,
            bundle_id: file
                .bundle_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(DEFAULT_BUNDLE_ID)
                .to_string(),
            sandbox: file.sandbox.unwrap_or(true),
        },
        source,
    ))
}

fn resolve_key_storage(file: &ApnsConfigFile) -> KeyStorage {
    match file
        .key_storage
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("keychain") => KeyStorage::Keychain,
        Some("file") => KeyStorage::File,
        _ if apns_keychain::load_apns_key_pem().is_some() => KeyStorage::Keychain,
        _ => KeyStorage::File,
    }
}

fn resolve_key_path(file: &ApnsConfigFile) -> Result<PathBuf> {
    let dir = apns_config_dir();
    if let Some(name) = file
        .key_file
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let path = Path::new(name);
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }
        return Ok(dir.join(name));
    }
    discover_auth_key_in_dir(&dir)
}

fn discover_auth_key_in_dir(dir: &Path) -> Result<PathBuf> {
    let mut matches = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("AuthKey_") && name.ends_with(".p8") {
                matches.push(entry.path());
            }
        }
    }
    match matches.len() {
        0 => anyhow::bail!(
            "no APNs key in {} — set keyStorage to keychain, keyFile in config.json, or place AuthKey_*.p8 there",
            dir.display()
        ),
        1 => Ok(matches.remove(0)),
        _ => anyhow::bail!(
            "multiple AuthKey_*.p8 files in {} — set keyFile in config.json",
            dir.display()
        ),
    }
}

fn env_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_sandbox_env() -> bool {
    std::env::var("MEDOUSA_APNS_SANDBOX")
        .ok()
        .map(|value| {
            let trimmed = value.trim();
            trimmed.eq_ignore_ascii_case("1") || trimmed.eq_ignore_ascii_case("true")
        })
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_key_file() {
        let base = Path::new("/tmp/medousa-apns-test");
        let file = ApnsConfigFile {
            team_id: "TEAM123".to_string(),
            key_id: "KEY456".to_string(),
            key_file: Some("AuthKey_ABCD1234.p8".to_string()),
            key_storage: Some("file".to_string()),
            bundle_id: None,
            sandbox: Some(true),
        };
        let resolved = resolve_key_path_in(base, &file).expect("resolve");
        assert_eq!(resolved, base.join("AuthKey_ABCD1234.p8"));
    }

    #[test]
    fn key_storage_prefers_explicit_keychain() {
        let file = ApnsConfigFile {
            team_id: "T".to_string(),
            key_id: "K".to_string(),
            key_file: Some("AuthKey_X.p8".to_string()),
            key_storage: Some("keychain".to_string()),
            bundle_id: None,
            sandbox: None,
        };
        assert_eq!(resolve_key_storage(&file), KeyStorage::Keychain);
    }

    fn resolve_key_path_in(base: &Path, file: &ApnsConfigFile) -> Result<PathBuf> {
        if let Some(name) = file.key_file.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            let path = Path::new(name);
            if path.is_absolute() {
                return Ok(path.to_path_buf());
            }
            return Ok(base.join(name));
        }
        discover_auth_key_in_dir(base)
    }
}
