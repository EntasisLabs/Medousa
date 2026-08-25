//! Typed secret I/O for daemon- and client-owned keyring services.
//!
//! Two reverse-DNS services only:
//! - `com.entasislabs.medousa.secrets.daemon`
//! - `com.entasislabs.medousa.secrets.client`
//!
//! Keyring accounts are the typed `v1/…` path. File fallback names use the
//! H02 opaque `storage_key()` — never a nested `v1/` directory tree.
//! Hermetic tests (`MEDOUSA_TEST_HERMETIC=1`) refuse the host keyring.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use medousa_types::secrets::{ClientSecretPath, DaemonSecretPath, InstallationId};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub const DAEMON_SERVICE: &str = "com.entasislabs.medousa.secrets.daemon";
pub const CLIENT_SERVICE: &str = "com.entasislabs.medousa.secrets.client";

const INSTALLATION_FILE: &str = "installation.json";
const SECRETS_DIR: &str = "secrets";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct InstallationDocument {
    installation_id: String,
}

/// Ensure `{dataDir}/installation.json` exists; create a UUID v4 on first boot.
pub fn ensure_installation_id(data_dir: &Path) -> Result<InstallationId> {
    let path = data_dir.join(INSTALLATION_FILE);
    if path.is_file() {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
        let doc: InstallationDocument = serde_json::from_str(&raw)
            .with_context(|| format!("parse {}", path.display()))?;
        return InstallationId::parse(&doc.installation_id)
            .map_err(|err| anyhow::anyhow!("{err}"));
    }
    fs::create_dir_all(data_dir).with_context(|| format!("create {}", data_dir.display()))?;
    let id = uuid::Uuid::new_v4().to_string();
    let installation_id = InstallationId::parse(&id).map_err(|err| anyhow::anyhow!("{err}"))?;
    let doc = InstallationDocument {
        installation_id: installation_id.as_str().to_string(),
    };
    let bytes = serde_json::to_vec_pretty(&doc)?;
    replace_private_file(&path, &bytes)?;
    Ok(installation_id)
}

pub fn load_installation_id(data_dir: &Path) -> Result<Option<InstallationId>> {
    let path = data_dir.join(INSTALLATION_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("read {}", path.display()))?;
    let doc: InstallationDocument = serde_json::from_str(&raw)
        .with_context(|| format!("parse {}", path.display()))?;
    InstallationId::parse(&doc.installation_id)
        .map(Some)
        .map_err(|err| anyhow::anyhow!("{err}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretBackend {
    Keyring,
    FileFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRead {
    pub value: String,
    pub backend: SecretBackend,
}

pub fn load_daemon_secret(data_dir: &Path, path: &DaemonSecretPath) -> Result<Option<SecretRead>> {
    load_secret(data_dir, DAEMON_SERVICE, &path.account(), path.storage_key().as_str())
}

pub fn save_daemon_secret(
    data_dir: &Path,
    path: &DaemonSecretPath,
    value: &str,
) -> Result<SecretBackend> {
    save_secret(
        data_dir,
        DAEMON_SERVICE,
        &path.account(),
        path.storage_key().as_str(),
        value,
    )
}

pub fn delete_daemon_secret(data_dir: &Path, path: &DaemonSecretPath) -> Result<()> {
    delete_secret(
        data_dir,
        DAEMON_SERVICE,
        &path.account(),
        path.storage_key().as_str(),
    )
}

pub fn daemon_secret_present(data_dir: &Path, path: &DaemonSecretPath) -> Result<bool> {
    Ok(load_daemon_secret(data_dir, path)?.is_some())
}

pub fn load_client_secret(data_dir: &Path, path: &ClientSecretPath) -> Result<Option<SecretRead>> {
    load_secret(data_dir, CLIENT_SERVICE, &path.account(), path.storage_key().as_str())
}

pub fn save_client_secret(
    data_dir: &Path,
    path: &ClientSecretPath,
    value: &str,
) -> Result<SecretBackend> {
    save_secret(
        data_dir,
        CLIENT_SERVICE,
        &path.account(),
        path.storage_key().as_str(),
        value,
    )
}

pub fn delete_client_secret(data_dir: &Path, path: &ClientSecretPath) -> Result<()> {
    delete_secret(
        data_dir,
        CLIENT_SERVICE,
        &path.account(),
        path.storage_key().as_str(),
    )
}

/// Prove that the platform keyring can write, read, and delete a client-owned
/// credential without using Medousa's file fallback.
///
/// This diagnostic never returns or logs its random probe value. It is intended
/// for release qualification on Apple targets before embedded inference
/// credentials are admitted.
pub fn probe_client_keyring_roundtrip() -> Result<()> {
    refuse_host_keyring()?;
    let probe_id = uuid::Uuid::new_v4();
    let account = format!("v1/diagnostics/keyring-roundtrip/{probe_id}");
    let value = format!("medousa-keyring-probe-{probe_id}");

    write_keyring(CLIENT_SERVICE, &account, &value).context("write client keyring diagnostic")?;

    let read_result = read_keyring(CLIENT_SERVICE, &account);
    let delete_result = delete_keyring(CLIENT_SERVICE, &account);

    let loaded = read_result.context("read client keyring diagnostic")?;
    if loaded.as_deref() != Some(value.as_str()) {
        let _ = delete_result;
        bail!("client keyring diagnostic read-back mismatch");
    }

    delete_result.context("delete client keyring diagnostic")?;
    if read_keyring(CLIENT_SERVICE, &account)
        .context("verify client keyring diagnostic deletion")?
        .is_some()
    {
        bail!("client keyring diagnostic remained after deletion");
    }

    Ok(())
}

/// Low-level legacy keyring read (migration only). Refuses host keyring when hermetic.
pub fn load_legacy_keyring(service: &str, account: &str) -> Result<Option<String>> {
    if hermetic() {
        return Ok(None);
    }
    let entry = match keyring::Entry::new(service, account) {
        Ok(entry) => entry,
        Err(_) => return Ok(None),
    };
    match entry.get_password() {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).context("read legacy keyring entry"),
    }
}

/// Low-level legacy keyring delete (migration only).
pub fn delete_legacy_keyring(service: &str, account: &str) -> Result<()> {
    if hermetic() {
        return Ok(());
    }
    let entry = match keyring::Entry::new(service, account) {
        Ok(entry) => entry,
        Err(_) => return Ok(()),
    };
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error).context("delete legacy keyring entry"),
    }
}

pub fn load_legacy_file(path: &Path) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let value = fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed))
    }
}

pub fn delete_legacy_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("delete {}", path.display())),
    }
}

fn load_secret(
    data_dir: &Path,
    service: &str,
    account: &str,
    storage_key: &str,
) -> Result<Option<SecretRead>> {
    if let Some(value) = read_keyring(service, account)? {
        return Ok(Some(SecretRead {
            value,
            backend: SecretBackend::Keyring,
        }));
    }
    let file_path = secret_file_path(data_dir, storage_key);
    if let Some(value) = load_legacy_file(&file_path)? {
        return Ok(Some(SecretRead {
            value,
            backend: SecretBackend::FileFallback,
        }));
    }
    Ok(None)
}

fn save_secret(
    data_dir: &Path,
    service: &str,
    account: &str,
    storage_key: &str,
    value: &str,
) -> Result<SecretBackend> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("secret value must not be empty");
    }
    // Some hosts accept a keyring write that is not readable afterward (denied
    // ACL, mangled account). Only claim Keyring after a successful read-back.
    if write_keyring(service, account, trimmed).is_ok()
        && read_keyring(service, account)?.as_deref() == Some(trimmed)
    {
        let _ = delete_legacy_file(&secret_file_path(data_dir, storage_key));
        return Ok(SecretBackend::Keyring);
    }
    let secrets_dir = data_dir.join(SECRETS_DIR);
    create_private_dir(&secrets_dir)?;
    replace_private_file(&secret_file_path(data_dir, storage_key), trimmed.as_bytes())?;
    Ok(SecretBackend::FileFallback)
}

fn delete_secret(
    data_dir: &Path,
    service: &str,
    account: &str,
    storage_key: &str,
) -> Result<()> {
    let _ = delete_keyring(service, account);
    delete_legacy_file(&secret_file_path(data_dir, storage_key))?;
    Ok(())
}

fn secret_file_path(data_dir: &Path, storage_key: &str) -> PathBuf {
    data_dir.join(SECRETS_DIR).join(storage_key)
}

fn hermetic() -> bool {
    std::env::var_os("MEDOUSA_TEST_HERMETIC").is_some()
}

fn refuse_host_keyring() -> Result<()> {
    if hermetic() {
        bail!("hermetic suite refuses host keyring");
    }
    Ok(())
}

fn read_keyring(service: &str, account: &str) -> Result<Option<String>> {
    if hermetic() {
        return Ok(None);
    }
    let entry = match keyring::Entry::new(service, account) {
        Ok(entry) => entry,
        Err(_) => return Ok(None),
    };
    match entry.get_password() {
        Ok(value) => {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).context("read keyring entry"),
    }
}

fn write_keyring(service: &str, account: &str, value: &str) -> Result<()> {
    refuse_host_keyring()?;
    let entry =
        keyring::Entry::new(service, account).context("open keyring entry for write")?;
    entry
        .set_password(value)
        .context("write keyring entry")?;
    Ok(())
}

fn delete_keyring(service: &str, account: &str) -> Result<()> {
    if hermetic() {
        return Ok(());
    }
    let entry = match keyring::Entry::new(service, account) {
        Ok(entry) => entry,
        Err(_) => return Ok(()),
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

fn replace_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("secret path has no file name")?;
    let temporary = path.with_file_name(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    create_private_file(&temporary, bytes)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error).with_context(|| format!("replace {}", path.display()));
    }
    Ok(())
}

fn create_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
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

/// Zeroize helper for callers that hold secret strings briefly.
pub fn zeroize_string(value: &mut String) {
    value.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;
    use medousa_types::secrets::{ConnectionId, IntegrationSecretSlot};

    #[test]
    fn installation_id_persists() {
        let dir = tempfile::tempdir().unwrap();
        let first = ensure_installation_id(dir.path()).unwrap();
        let second = ensure_installation_id(dir.path()).unwrap();
        assert_eq!(first, second);
        assert!(dir.path().join(INSTALLATION_FILE).is_file());
    }

    #[test]
    fn file_fallback_round_trip_when_hermetic() {
        // Hermetic refuses keyring writes → file fallback.
        unsafe {
            std::env::set_var("MEDOUSA_TEST_HERMETIC", "1");
        }
        let dir = tempfile::tempdir().unwrap();
        let installation = ensure_installation_id(dir.path()).unwrap();
        let connection =
            ConnectionId::parse("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        let path = DaemonSecretPath::Integration {
            installation_id: installation,
            connection_id: connection,
            slot: IntegrationSecretSlot::ApiKey,
        };
        let backend = save_daemon_secret(dir.path(), &path, "sk-test").unwrap();
        assert_eq!(backend, SecretBackend::FileFallback);
        let loaded = load_daemon_secret(dir.path(), &path).unwrap().unwrap();
        assert_eq!(loaded.value, "sk-test");
        assert_eq!(loaded.backend, SecretBackend::FileFallback);
        delete_daemon_secret(dir.path(), &path).unwrap();
        assert!(load_daemon_secret(dir.path(), &path).unwrap().is_none());
        unsafe {
            std::env::remove_var("MEDOUSA_TEST_HERMETIC");
        }
    }

    #[test]
    #[ignore = "touches the host OS keyring; run explicitly for release qualification"]
    fn host_keyring_roundtrip() {
        probe_client_keyring_roundtrip().unwrap();
    }
}
