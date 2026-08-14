//! Native local-client credentials shared by the daemon and first-party clients.
//!
//! The bearer secret is stored in the platform credential store when possible,
//! with an owner-only file fallback. The daemon retains only a SHA-256 verifier
//! and compares digests in constant time on the request path.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

pub const HOME_LOCAL_NAME: &str = "home-local";
const RECORD_VERSION: u8 = 1;
const KEYRING_SERVICE: &str = "com.entasislabs.medousa.local-credentials";
const CREDENTIALS_DIR: &str = "credentials";
const RECORD_FILE: &str = "home-local.json";
const SECRET_FILE: &str = "home-local.secret";

#[derive(Clone)]
pub struct LocalCredentialVerifier {
    credential_id: Arc<str>,
    digest: [u8; 32],
}

impl LocalCredentialVerifier {
    pub fn from_token(credential_id: impl Into<Arc<str>>, token: &str) -> Self {
        Self {
            credential_id: credential_id.into(),
            digest: Sha256::digest(token.as_bytes()).into(),
        }
    }

    pub fn credential_id(&self) -> &str {
        &self.credential_id
    }

    pub fn credential_id_arc(&self) -> Arc<str> {
        self.credential_id.clone()
    }

    pub fn verify(&self, token: &str) -> bool {
        constant_time_eq(&self.digest, &Sha256::digest(token.as_bytes()))
    }
}

pub struct LocalCredentialSecret {
    credential_id: String,
    token: String,
}

impl LocalCredentialSecret {
    pub fn credential_id(&self) -> &str {
        &self.credential_id
    }

    pub fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for LocalCredentialSecret {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CredentialRecord {
    version: u8,
    name: String,
    credential_id: String,
    token_sha256: String,
    secret_store: SecretStore,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SecretStore {
    Keyring { account: String },
    OwnerOnlyFile { relative_path: String },
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecretPayload {
    credential_id: String,
    token: String,
}

impl Drop for SecretPayload {
    fn drop(&mut self) {
        self.token.zeroize();
    }
}

/// Provision the stable Home credential if necessary and return its verifier.
///
/// This validates that the verifier and secret agree on every startup. Missing
/// or mismatched material fails closed instead of silently rotating authority.
pub fn provision_home_local(data_dir: &Path) -> Result<LocalCredentialVerifier> {
    let credentials_dir = data_dir.join(CREDENTIALS_DIR);
    create_private_dir(&credentials_dir)?;
    let record_path = credentials_dir.join(RECORD_FILE);

    if record_path.is_file() {
        let record = read_record(&record_path)?;
        let secret = load_secret_for_record(data_dir, &record)?;
        return verifier_from_parts(&record, &secret);
    }

    let account = keyring_account(data_dir)?;
    if let Ok(Some(secret)) = read_keyring_secret(&account) {
        let record = record_for_secret(
            secret.credential_id.clone(),
            &secret,
            SecretStore::Keyring { account },
        );
        create_record(&record_path, &record)?;
        return load_verifier(data_dir, &record_path);
    }

    let secret_path = credentials_dir.join(SECRET_FILE);
    if secret_path.is_file() {
        let secret = read_file_secret(&secret_path)?;
        let record = record_for_secret(
            secret.credential_id.clone(),
            &secret,
            SecretStore::OwnerOnlyFile {
                relative_path: format!("{CREDENTIALS_DIR}/{SECRET_FILE}"),
            },
        );
        create_record(&record_path, &record)?;
        return load_verifier(data_dir, &record_path);
    }

    let secret = generate_secret();
    let secret_store = match write_keyring_secret(&account, &secret) {
        Ok(()) => SecretStore::Keyring { account },
        Err(_) => {
            let mut encoded = serde_json::to_vec(&secret)?;
            let result = create_private_file(&secret_path, &encoded);
            encoded.zeroize();
            result?;
            SecretStore::OwnerOnlyFile {
                relative_path: format!("{CREDENTIALS_DIR}/{SECRET_FILE}"),
            }
        }
    };
    let record = record_for_secret(secret.credential_id.clone(), &secret, secret_store);
    create_record(&record_path, &record)?;
    load_verifier(data_dir, &record_path)
}

/// Load the Home bearer for a native first-party transport.
pub fn load_home_local_secret(data_dir: &Path) -> Result<LocalCredentialSecret> {
    let record_path = data_dir.join(CREDENTIALS_DIR).join(RECORD_FILE);
    let record = read_record(&record_path)?;
    let mut secret = load_secret_for_record(data_dir, &record)?;
    verifier_from_parts(&record, &secret)?;
    Ok(LocalCredentialSecret {
        credential_id: std::mem::take(&mut secret.credential_id),
        token: std::mem::take(&mut secret.token),
    })
}

fn load_verifier(data_dir: &Path, record_path: &Path) -> Result<LocalCredentialVerifier> {
    let record = read_record(record_path)?;
    let secret = load_secret_for_record(data_dir, &record)?;
    verifier_from_parts(&record, &secret)
}

fn generate_secret() -> SecretPayload {
    let mut token = [0u8; 32];
    OsRng.fill_bytes(&mut token);
    SecretPayload {
        credential_id: uuid::Uuid::new_v4().to_string(),
        token: URL_SAFE_NO_PAD.encode(token),
    }
}

fn record_for_secret(
    credential_id: String,
    secret: &SecretPayload,
    secret_store: SecretStore,
) -> CredentialRecord {
    CredentialRecord {
        version: RECORD_VERSION,
        name: HOME_LOCAL_NAME.to_string(),
        credential_id,
        token_sha256: digest_hex(&Sha256::digest(secret.token.as_bytes())),
        secret_store,
    }
}

fn verifier_from_parts(
    record: &CredentialRecord,
    secret: &SecretPayload,
) -> Result<LocalCredentialVerifier> {
    validate_record(record)?;
    if secret.credential_id != record.credential_id {
        bail!("local credential identifier does not match its verifier record");
    }
    let digest = decode_digest(&record.token_sha256)?;
    if !constant_time_eq(&digest, &Sha256::digest(secret.token.as_bytes())) {
        bail!("local credential secret does not match its verifier record");
    }
    Ok(LocalCredentialVerifier {
        credential_id: Arc::from(record.credential_id.as_str()),
        digest,
    })
}

fn validate_record(record: &CredentialRecord) -> Result<()> {
    if record.version != RECORD_VERSION {
        bail!(
            "unsupported local credential record version {}",
            record.version
        );
    }
    if record.name != HOME_LOCAL_NAME || record.credential_id.trim().is_empty() {
        bail!("invalid home-local credential record");
    }
    Ok(())
}

fn load_secret_for_record(data_dir: &Path, record: &CredentialRecord) -> Result<SecretPayload> {
    match &record.secret_store {
        SecretStore::Keyring { account } => read_keyring_secret(account)?.ok_or_else(|| {
            anyhow::anyhow!("home-local credential is missing from the platform credential store")
        }),
        SecretStore::OwnerOnlyFile { relative_path } => {
            let relative = Path::new(relative_path);
            if relative.is_absolute()
                || relative
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                bail!("invalid local credential secret path");
            }
            read_file_secret(&data_dir.join(relative))
        }
    }
}

fn read_record(path: &Path) -> Result<CredentialRecord> {
    let raw = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&raw).with_context(|| format!("parse {}", path.display()))
}

fn create_record(path: &Path, record: &CredentialRecord) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(record).context("serialize local credential record")?;
    match create_private_file(path, &encoded) {
        Ok(()) => Ok(()),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == ErrorKind::AlreadyExists) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn read_file_secret(path: &Path) -> Result<SecretPayload> {
    let mut raw = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let result = serde_json::from_slice(&raw).with_context(|| format!("parse {}", path.display()));
    raw.zeroize();
    result
}

fn read_keyring_secret(account: &str) -> Result<Option<SecretPayload>> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, account)
        .context("open local credential keyring entry")?;
    match entry.get_password() {
        Ok(mut raw) => {
            let result = serde_json::from_str(&raw)
                .map(Some)
                .context("parse local credential keyring entry");
            raw.zeroize();
            result
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error).context("read local credential keyring entry"),
    }
}

fn write_keyring_secret(account: &str, secret: &SecretPayload) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, account)
        .context("open local credential keyring entry")?;
    let mut encoded = serde_json::to_string(secret).context("serialize local credential secret")?;
    let result = entry
        .set_password(&encoded)
        .context("write local credential keyring entry");
    encoded.zeroize();
    result
}

fn keyring_account(data_dir: &Path) -> Result<String> {
    let absolute = if data_dir.is_absolute() {
        data_dir.to_path_buf()
    } else {
        std::env::current_dir()?.join(data_dir)
    };
    let digest = Sha256::digest(absolute.to_string_lossy().as_bytes());
    Ok(format!("{HOME_LOCAL_NAME}-{}", &digest_hex(&digest)[..24]))
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

fn decode_digest(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        bail!("invalid local credential verifier length");
    }
    let mut digest = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(pair)?;
        digest[index] =
            u8::from_str_radix(pair, 16).context("invalid local credential verifier")?;
    }
    Ok(digest)
}

fn digest_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(encoded, "{byte:02x}");
    }
    encoded
}

fn constant_time_eq(left: &[u8; 32], right: &[u8]) -> bool {
    if right.len() != left.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "medousa-local-credential-{name}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn provision_file_fixture(data_dir: &Path) -> Result<LocalCredentialSecret> {
        let credentials = data_dir.join(CREDENTIALS_DIR);
        create_private_dir(&credentials)?;
        let secret = generate_secret();
        let mut encoded = serde_json::to_vec(&secret)?;
        create_private_file(&credentials.join(SECRET_FILE), &encoded)?;
        encoded.zeroize();
        let record = record_for_secret(
            secret.credential_id.clone(),
            &secret,
            SecretStore::OwnerOnlyFile {
                relative_path: format!("{CREDENTIALS_DIR}/{SECRET_FILE}"),
            },
        );
        create_record(&credentials.join(RECORD_FILE), &record)?;
        load_home_local_secret(data_dir)
    }

    #[test]
    fn file_secret_is_stable_and_verifies_without_string_allocation() -> Result<()> {
        let data_dir = test_dir("stable");
        let secret = provision_file_fixture(&data_dir)?;
        let first = provision_home_local(&data_dir)?;
        let second = provision_home_local(&data_dir)?;
        assert_eq!(first.credential_id(), second.credential_id());
        assert_eq!(first.credential_id(), secret.credential_id());
        assert!(first.verify(secret.token()));
        assert!(!first.verify("wrong-token"));
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[test]
    fn generated_secret_has_at_least_256_bits() {
        let secret = generate_secret();
        assert_eq!(URL_SAFE_NO_PAD.decode(&secret.token).unwrap().len(), 32);
    }

    #[test]
    fn mismatched_secret_fails_closed() -> Result<()> {
        let data_dir = test_dir("mismatch");
        let secret = provision_file_fixture(&data_dir)?;
        let path = data_dir.join(CREDENTIALS_DIR).join(SECRET_FILE);
        let replacement = SecretPayload {
            credential_id: secret.credential_id().to_string(),
            token: "replacement".to_string(),
        };
        fs::write(&path, serde_json::to_vec(&replacement)?)?;
        assert!(provision_home_local(&data_dir).is_err());
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn file_fallback_is_owner_only() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let data_dir = test_dir("permissions");
        let _ = provision_file_fixture(&data_dir)?;
        let credentials = data_dir.join(CREDENTIALS_DIR);
        assert_eq!(
            fs::metadata(&credentials)?.permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(credentials.join(SECRET_FILE))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(credentials.join(RECORD_FILE))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        fs::remove_dir_all(data_dir)?;
        Ok(())
    }
}
