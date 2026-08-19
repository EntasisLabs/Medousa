//! Two-service secret store: daemon-owned and client-owned keyring accounts.
//!
//! Keyring accounts are typed `v1/…` paths. File fallbacks use opaque H02
//! storage keys. `MEDOUSA_TEST_HERMETIC=1` refuses the host keyring.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use medousa_types::{
    ClientSecretPath, DaemonSecretPath, InstallationId, StorageAuthorityKey,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub const DAEMON_SERVICE: &str = "com.entasislabs.medousa.secrets.daemon";
pub const CLIENT_SERVICE: &str = "com.entasislabs.medousa.secrets.client";

const INSTALLATION_FILE: &str = "installation.json";
const SECRETS_DIR: &str = "secrets";

/// Legacy OS keyring services retired by the two-service cutover.
pub const LEGACY_SERVICES: &[&str] = &[
    "medousa.tui",
    "medousa.providers",
    "medousa.stt",
    "medousa.discord",
    "medousa.telegram",
    "medousa.slack",
    "medousa.surreal",
    "medousa.chatgpt",
    "medousa.apns",
    "medousa.pairing",
    "com.entasislabs.medousa.local-credentials",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretBackend {
    Keyring,
    OwnerOnlyFile,
}

#[derive(Debug, Serialize, Deserialize)]
struct InstallationDocument {
    installation_id: String,
}

#[derive(Clone, Debug)]
pub struct SecretStore {
    data_dir: PathBuf,
}

impl SecretStore {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn ensure_installation_id(&self) -> Result<InstallationId> {
        let path = self.data_dir.join(INSTALLATION_FILE);
        if path.is_file() {
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("read {}", path.display()))?;
            let document: InstallationDocument = serde_json::from_str(&raw)
                .with_context(|| format!("parse {}", path.display()))?;
            return InstallationId::parse(&document.installation_id)
                .map_err(|err| anyhow::anyhow!(err));
        }
        let installation = InstallationId::generate();
        let document = InstallationDocument {
            installation_id: installation.as_str().to_string(),
        };
        let encoded = serde_json::to_vec_pretty(&document)?;
        create_private_dir(&self.data_dir)?;
        create_private_file(&path, &encoded)?;
        Ok(installation)
    }

    pub fn get_daemon(&self, path: &DaemonSecretPath) -> Option<String> {
        read_secret(DAEMON_SERVICE, &path.keyring_account(), &self.file_path(path.storage_key()))
    }

    pub fn daemon_configured(&self, path: &DaemonSecretPath) -> bool {
        self.get_daemon(path).is_some()
    }

    pub fn set_daemon(&self, path: &DaemonSecretPath, value: Option<&str>) -> Result<SecretBackend> {
        write_secret(
            DAEMON_SERVICE,
            &path.keyring_account(),
            &self.file_path(path.storage_key()),
            value,
        )
    }

    pub fn get_client(&self, path: &ClientSecretPath) -> Option<String> {
        read_secret(CLIENT_SERVICE, &path.keyring_account(), &self.file_path(path.storage_key()))
    }

    pub fn set_client(&self, path: &ClientSecretPath, value: Option<&str>) -> Result<SecretBackend> {
        write_secret(
            CLIENT_SERVICE,
            &path.keyring_account(),
            &self.file_path(path.storage_key()),
            value,
        )
    }

    pub fn delete_legacy_entry(&self, service: &str, account: &str) -> Result<()> {
        delete_keyring(service, account)
    }

    pub fn delete_legacy_file(&self, relative: &str) -> Result<()> {
        let path = self.data_dir.join(relative);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| format!("delete {}", path.display())),
        }
    }

    fn file_path(&self, key: StorageAuthorityKey) -> PathBuf {
        self.data_dir.join(SECRETS_DIR).join(key.as_str())
    }
}

pub fn hermetic() -> bool {
    std::env::var_os("MEDOUSA_TEST_HERMETIC").is_some()
}

fn refuse_host_keyring() -> Result<(), keyring::Error> {
    if hermetic() {
        return Err(keyring::Error::NoEntry);
    }
    Ok(())
}

fn keyring_entry(service: &str, account: &str) -> Result<keyring::Entry, keyring::Error> {
    refuse_host_keyring()?;
    keyring::Entry::new(service, account)
}

fn read_secret(service: &str, account: &str, file_path: &Path) -> Option<String> {
    if let Ok(entry) = keyring_entry(service, account)
        && let Ok(mut value) = entry.get_password()
    {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let owned = trimmed.to_string();
            value.zeroize();
            return Some(owned);
        }
        value.zeroize();
    }
    fs::read_to_string(file_path)
        .ok()
        .and_then(|mut value| {
            let trimmed = value.trim();
            let owned = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
            value.zeroize();
            owned
        })
}

fn write_secret(
    service: &str,
    account: &str,
    file_path: &Path,
    value: Option<&str>,
) -> Result<SecretBackend> {
    match value.map(str::trim).filter(|token| !token.is_empty()) {
        Some(token) => {
            if let Ok(entry) = keyring_entry(service, account)
                && entry.set_password(token).is_ok()
            {
                let _ = fs::remove_file(file_path);
                return Ok(SecretBackend::Keyring);
            }
            if let Some(parent) = file_path.parent() {
                create_private_dir(parent)?;
            }
            replace_private_file(file_path, token.as_bytes())?;
            Ok(SecretBackend::OwnerOnlyFile)
        }
        None => {
            let _ = delete_keyring(service, account);
            match fs::remove_file(file_path) {
                Ok(()) | Err(_) => {}
            }
            Ok(SecretBackend::Keyring)
        }
    }
}

fn delete_keyring(service: &str, account: &str) -> Result<()> {
    let Ok(entry) = keyring_entry(service, account) else {
        return Ok(());
    };
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error).context("delete keyring entry"),
    }
}

fn create_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("restrict {}", path.display()))?;
    }
    Ok(())
}

fn create_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("sync {}", path.display()))?;
    Ok(())
}

fn replace_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("secret path has no file name")?;
    let temporary = path.with_file_name(format!(
        ".{file_name}.{}.tmp",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    {
        let mut file = options
            .open(&temporary)
            .with_context(|| format!("create {}", temporary.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("write {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("sync {}", temporary.display()))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        if error.kind() == ErrorKind::AlreadyExists || path.exists() {
            fs::write(path, bytes).with_context(|| format!("replace {}", path.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
            }
            return Ok(());
        }
        return Err(error).with_context(|| format!("replace {}", path.display()));
    }
    Ok(())
}

pub fn read_legacy_keyring(service: &str, account: &str) -> Option<String> {
    if let Ok(entry) = keyring_entry(service, account)
        && let Ok(mut value) = entry.get_password()
    {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let owned = trimmed.to_string();
            value.zeroize();
            return Some(owned);
        }
        value.zeroize();
    }
    None
}

pub fn read_legacy(service: &str, account: &str, file_path: &Path) -> Option<String> {
    read_legacy_keyring(service, account).or_else(|| {
        fs::read_to_string(file_path)
            .ok()
            .and_then(|mut value| {
                let trimmed = value.trim();
                let owned = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                value.zeroize();
                owned
            })
    })
}

pub fn delete_legacy(service: &str, account: &str, file_path: Option<&Path>) -> Result<()> {
    delete_keyring(service, account)?;
    if let Some(path) = file_path {
        match fs::remove_file(path) {
            Ok(()) | Err(_) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::{ConnectionId, IntegrationSecretSlot};

    fn test_dir() -> PathBuf {
        std::env::temp_dir().join(format!("medousa-secrets-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn installation_id_is_stable() {
        let dir = test_dir();
        let store = SecretStore::new(&dir);
        let first = store.ensure_installation_id().unwrap();
        let second = store.ensure_installation_id().unwrap();
        assert_eq!(first, second);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn hermetic_writes_owner_only_files() {
        let dir = test_dir();
        let store = SecretStore::new(&dir);
        let installation = store.ensure_installation_id().unwrap();
        let connection = ConnectionId::generate();
        let path = DaemonSecretPath::integration(
            installation,
            connection,
            IntegrationSecretSlot::ApiKey,
        );
        let previous = std::env::var_os("MEDOUSA_TEST_HERMETIC");
        unsafe { std::env::set_var("MEDOUSA_TEST_HERMETIC", "1") };
        let backend = store.set_daemon(&path, Some("sk-test")).unwrap();
        match previous {
            Some(value) => unsafe { std::env::set_var("MEDOUSA_TEST_HERMETIC", value) },
            None => unsafe { std::env::remove_var("MEDOUSA_TEST_HERMETIC") },
        }
        assert_eq!(backend, SecretBackend::OwnerOnlyFile);
        assert_eq!(store.get_daemon(&path).as_deref(), Some("sk-test"));
        let file = dir.join("secrets").join(path.storage_key().as_str());
        assert!(file.exists());
        assert!(!file.file_name().unwrap().to_string_lossy().contains("api_key"));
        let _ = fs::remove_dir_all(dir);
    }
}
